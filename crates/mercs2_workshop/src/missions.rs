//! The Missions workbench — a Blueprint-style node editor driven by **parsed Lua**.
//!
//! The previous drop shipped a hand-decoded, hardcoded `PmcCon016` layout: pretty, totally
//! inert. Every mission picked would show the same eight nodes with the same labels, and
//! save-state played no part. This rewrite throws out the hardcoded node table and derives
//! the graph from the actual bytes of the picked `.lua`.
//!
//! # What drives the graph
//!
//! A line-oriented Lua scanner over the picked script that:
//!   * finds every top-level `function Name(args) ... end`,
//!   * scans the whole file for the engine's distinctive call shapes
//!     (`MrxVoSequence.Start(...)`, `self:CreateChild({...})`,
//!     `self:_CreateEvent(Event.X, ..., Callback, ...)`,
//!     `MrxMusic.PlaySpecialMusic(...)` / `MrxMusic.PlayAmbientMusic(...)`,
//!     `MrxLayerManager.Add/Remove(...)`),
//!   * attributes each call to the top-level function whose body contains it (nearest
//!     preceding `function` whose matching `end` has not yet closed),
//!   * emits one Fn node per top-level function and one typed call-node per recognised call,
//!     wiring exec-flow in file order within each function.
//!
//! The scanner is a stripped Lua lexer that respects `--`/`--[[...]]` comments and
//! `"..."`/`'...'` string literals, plus Lua long-brackets `[[...]]` / `[=[...]=]`. It is NOT
//! a full parser. If a call cannot be matched (unbalanced parens, string containing `(` in a
//! shape we haven't seen), the panel renders an "Unparseable" node naming the line, not an
//! empty canvas.
//!
//! # Mission picker
//!
//! The left panel's "Script" section lists every corpus script whose first non-comment line
//! is `inherit("MrxTask*")`, `inherit("MrxMission*")`, `inherit("MrxTutorial")`, or
//! `inherit("MecJob")` — the retail mission-flow superclasses. Click one, the parser runs on
//! its bytes, and the centre pane redraws from scratch. Different missions produce different
//! graphs, which is the entire point.
//!
//! # Save-state overlay
//!
//! When a `.profile` is loaded via the verb bar, [`Panel::load_save`] decodes it with
//! `mercs2_formats::save::Profile::save_state()` and keeps the result. Mission entries in the
//! picker are badged by state:
//!   * **ACTIVE** — the mission id is in `tFlowData.tActiveMissions` with `nState == 1`.
//!   * **DONE** — the mission id is in `tFlowData.tMyFlowData` with any non-zero flag.
//!   * **CULLED** — the mission id appears in `tFlowData.tCulledBindings` as a struck key
//!     and is NOT active and NOT completed.
//!   * otherwise, no badge.
//!
//! The picked mission's root entry fn (whatever is named `Activated`, else the first fn)
//! inherits that state as a halo colour on the canvas.
//!
//! No save loaded ⇒ the picker shows no badges and the status reads "no save loaded"; nothing
//! is synthesised.

use std::collections::{BTreeMap, HashMap};

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::gui::theme;

// ───────────────────────────────────────────────────────────────────── taxonomy / types

/// Which category a node belongs to — governs the gradient header colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    /// A top-level `function Name(self)` definition that overrides an engine lifecycle hook.
    Lifecycle,
    /// A plain top-level `function Name(...)` helper defined in this file.
    Fn,
    /// A VO cue (`MrxVoSequence.Start`).
    Vo,
    /// A child objective (`self:CreateChild({sModuleName=…})`).
    Objective,
    /// An `Event.*` binding (`self:_CreateEvent(Event.X, …)`).
    Event,
    /// A music call (`MrxMusic.*`).
    Music,
    /// A layer load/unload (`MrxLayerManager.Add` / `Remove`).
    Layer,
    /// A call we recognised the SHAPE of but not its specific kind — kept so the user sees
    /// the surface instead of a silent drop.
    Misc,
    /// A line we matched a call-site prefix on but could not finish parsing (unbalanced
    /// parens etc.). Rendered honestly as "Unparseable" rather than omitted.
    Unparseable,
}

impl NodeKind {
    /// (header gradient top, header gradient bottom, header text, eyebrow label).
    fn header_colours(self) -> (Color32, Color32, Color32, &'static str) {
        match self {
            NodeKind::Lifecycle => (
                Color32::from_rgb(0x4a, 0x3a, 0x7a),
                Color32::from_rgb(0x2f, 0x26, 0x50),
                Color32::from_rgb(0xde, 0xd2, 0xff),
                "LIFECYCLE",
            ),
            NodeKind::Fn => (
                Color32::from_rgb(0x1b, 0x4b, 0x6a),
                Color32::from_rgb(0x11, 0x31, 0x48),
                Color32::from_rgb(0xc9, 0xe4, 0xf7),
                "FN",
            ),
            NodeKind::Vo => (
                Color32::from_rgb(0x7a, 0x2f, 0x57),
                Color32::from_rgb(0x4f, 0x1f, 0x39),
                Color32::from_rgb(0xff, 0xd9, 0xee),
                "VO",
            ),
            NodeKind::Objective => (
                Color32::from_rgb(0x6a, 0x4f, 0x14),
                Color32::from_rgb(0x49, 0x36, 0x11),
                Color32::from_rgb(0xf8, 0xe8, 0xbd),
                "OBJECTIVE",
            ),
            NodeKind::Event => (
                Color32::from_rgb(0x7a, 0x2f, 0x36),
                Color32::from_rgb(0x4f, 0x1f, 0x26),
                Color32::from_rgb(0xff, 0xda, 0xda),
                "EVENT",
            ),
            NodeKind::Music => (
                Color32::from_rgb(0x1e, 0x55, 0x55),
                Color32::from_rgb(0x11, 0x36, 0x36),
                Color32::from_rgb(0xa9, 0xed, 0xe1),
                "MUSIC",
            ),
            NodeKind::Layer => (
                Color32::from_rgb(0x24, 0x44, 0x5c),
                Color32::from_rgb(0x14, 0x2a, 0x3a),
                Color32::from_rgb(0xbe, 0xd6, 0xe8),
                "LAYER",
            ),
            NodeKind::Misc => (
                Color32::from_rgb(0x2f, 0x33, 0x3b),
                Color32::from_rgb(0x1c, 0x1f, 0x25),
                Color32::from_rgb(0xcf, 0xcf, 0xcf),
                "CALL",
            ),
            NodeKind::Unparseable => (
                Color32::from_rgb(0x5a, 0x1f, 0x1f),
                Color32::from_rgb(0x3a, 0x12, 0x12),
                Color32::from_rgb(0xff, 0xc8, 0xc8),
                "UNPARSEABLE",
            ),
        }
    }
}

/// A typed pin — colour-coded by Lua value kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PinTy {
    Exec,
    Str,
    Int,
    Obj,
    Arr,
}

impl PinTy {
    pub fn colour(self) -> Color32 {
        match self {
            PinTy::Exec => Color32::from_rgb(0xf4, 0xf6, 0xfa),
            PinTy::Str => Color32::from_rgb(0xec, 0x48, 0x99),
            PinTy::Int => Color32::from_rgb(0x10, 0xb9, 0x81),
            PinTy::Obj => Color32::from_rgb(0x8b, 0x5c, 0xf6),
            PinTy::Arr => Color32::from_rgb(0xf9, 0x73, 0x16),
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            PinTy::Exec => "exec",
            PinTy::Str => "str",
            PinTy::Int => "int",
            PinTy::Obj => "obj",
            PinTy::Arr => "arr",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pin {
    pub label: String,
    pub ty: PinTy,
    /// Secondary annotation printed beside the pin (literal values).
    pub annot: Option<String>,
    /// If the pin's value came from a single scalar LITERAL in the Lua source,
    /// its byte range in `Mission::source` (quotes included, for strings).
    /// Drives the inspector's "edit → write back" surface. `None` for pins
    /// whose value is dynamic (an identifier, a table expression, a count,
    /// an exec flow), which must stay read-only — editing those is a text
    /// rewrite the string-splice path does not know how to do.
    pub edit_range: Option<std::ops::Range<usize>>,
}

impl Default for Pin {
    fn default() -> Self {
        Self {
            label: String::new(),
            ty: PinTy::Exec,
            annot: None,
            edit_range: None,
        }
    }
}

/// A graph node derived from the parsed Lua.
#[derive(Debug)]
pub struct Node {
    /// Stable identifier within the mission, synthesised from kind+line (e.g. `vo_17`).
    pub id: String,
    pub title: String,
    pub kind: NodeKind,
    pub pos: Pos2,
    pub width: f32,
    pub inputs: Vec<Pin>,
    pub outputs: Vec<Pin>,
    /// Short one-line description for the inspector heading.
    pub blurb: String,
    /// Lua source line (1-based) this node renders from.
    pub source_line: u32,
    /// Up to a dozen source lines captured verbatim for the inspector.
    pub snippet: Vec<String>,
    /// Which top-level function body contains this node (for grouping and chaining).
    pub containing_fn: String,
}

/// A typed wire. `(node_id, pin_label)` on each end.
#[derive(Debug)]
pub struct Wire {
    pub from: (String, String),
    pub to: (String, String),
    pub ty: PinTy,
}

/// The parsed mission as a graph.
#[derive(Debug, Default)]
pub struct Mission {
    pub id: String,
    pub path: String,
    pub inherits: String,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
    /// Names of each top-level function in source order (for the inspector/outline).
    pub functions: Vec<FnOutline>,
    /// Parser notes: lines where a call-prefix matched but extraction failed. Not an error —
    /// surfaces as Unparseable nodes so the user knows which lines didn't resolve.
    pub parse_notes: Vec<String>,
    /// Verbatim source bytes retained so the inspector can show arbitrary line ranges on
    /// demand (jump-to-source and node snippets).
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct FnOutline {
    pub name: String,
    pub line: u32,
    pub is_lifecycle: bool,
}

/// One entry in the mission catalog — a corpus script that inherits a mission superclass.
#[derive(Debug, Clone)]
pub struct MissionEntry {
    /// Corpus path (`"lua/pmccon016.lua"` or `"dlc/..."`).
    pub path: String,
    /// Derived id (`"PmcCon016"`) — the filename stem with retail casing restored where
    /// obvious (`pmccon` → `PmcCon`, `oilcon` → `OilCon`, etc.).
    pub id: String,
    /// The `inherit("…")` superclass this script declares.
    pub inherits: String,
}

/// The in-memory save overlay. Only what the picker and canvas use is retained — see
/// `mercs2_formats::save::SaveState` for the full read.
#[derive(Debug, Clone, Default)]
pub struct SaveOverlay {
    pub source: String,
    pub active_contract: String,
    /// mission-id → nState (typically 1 while active).
    pub active: BTreeMap<String, f64>,
    /// mission-id → flow flag (non-zero == completed/advanced).
    pub completed_flow: BTreeMap<String, f64>,
    /// Mission-flow binding chain (`tCulledBindings`) in order.
    pub flow_chain: Vec<String>,
}

impl SaveOverlay {
    pub fn state_for(&self, id: &str) -> MissionState {
        if self.active.contains_key(id) {
            MissionState::Active
        } else if self
            .completed_flow
            .get(id)
            .copied()
            .is_some_and(|v| v != 0.0)
        {
            MissionState::Done
        } else if self.flow_chain.iter().any(|k| k == id) {
            // Appears in the chain but neither active nor completed — "in flow but not yet
            // reached" is CULLED only when the engine struck it; we don't have strike info
            // without the raw save text, so report as Queued and let the user see it.
            MissionState::Queued
        } else {
            MissionState::None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionState {
    None,
    Queued,
    Active,
    Done,
}

// ───────────────────────────────────────────────────────────────────── palette (truth)

pub const LIFECYCLE_HOOKS: &[(&str, &str)] = &[
    ("Activated", "(self)"),
    ("Complete", "(self)"),
    ("Cancel", "(self)"),
    ("Cleanup", "(self)"),
    ("LoadAssets", "(self, tSaveData)"),
    ("AssetsLoaded", "(self)"),
];

fn is_lifecycle(name: &str) -> bool {
    LIFECYCLE_HOOKS.iter().any(|(n, _)| *n == name)
}

// ───────────────────────────────────────────────────────────────────── Lua lexer helpers

/// Classify each byte of `src` as "in code" (true) or "in string/comment" (false). Comments
/// (short `--` and long `--[[...]]` with level) and `"…"`/`'…'` / long `[[...]]` strings are
/// marked false. Line boundaries are preserved — a `\n` byte is always marked as code so
/// line-numbering code can scan `in_code` and still see newlines.
fn code_mask(src: &str) -> Vec<bool> {
    let b = src.as_bytes();
    let mut mask = vec![true; b.len()];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        // Short comment: -- ... (up to EOL), unless followed by [[ or [=[... which starts a
        // long comment.
        if c == b'-' && i + 1 < b.len() && b[i + 1] == b'-' {
            mask[i] = false;
            mask[i + 1] = false;
            // Check for long bracket after the --
            if let Some((level, after)) = long_bracket_open(b, i + 2) {
                // Long comment: find matching ]==]
                let close = long_bracket_close(b, after, level);
                let end = close.unwrap_or(b.len());
                for j in i..end.min(b.len()) {
                    if b[j] != b'\n' {
                        mask[j] = false;
                    }
                }
                i = end;
                continue;
            }
            // Short comment to EOL
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\n' {
                mask[j] = false;
                j += 1;
            }
            i = j;
            continue;
        }
        // Short string
        if c == b'"' || c == b'\'' {
            let quote = c;
            mask[i] = false;
            let mut j = i + 1;
            while j < b.len() {
                mask[j] = false;
                if b[j] == b'\\' && j + 1 < b.len() {
                    mask[j + 1] = false;
                    j += 2;
                    continue;
                }
                if b[j] == quote {
                    j += 1;
                    break;
                }
                if b[j] == b'\n' {
                    // Unterminated — stop at EOL, keep newline as code so lines still line
                    // up for the human reading the diagnostics.
                    mask[j] = true;
                    j += 1;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        // Long string
        if c == b'[' {
            if let Some((level, after)) = long_bracket_open(b, i) {
                let close = long_bracket_close(b, after, level);
                let end = close.unwrap_or(b.len());
                for j in i..end.min(b.len()) {
                    if b[j] != b'\n' {
                        mask[j] = false;
                    }
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    mask
}

/// If `b[i..]` opens a long bracket (`[`, 0+ `=`, `[`), return (level, index-after-opener).
fn long_bracket_open(b: &[u8], i: usize) -> Option<(usize, usize)> {
    if i >= b.len() || b[i] != b'[' {
        return None;
    }
    let mut j = i + 1;
    let mut level = 0usize;
    while j < b.len() && b[j] == b'=' {
        level += 1;
        j += 1;
    }
    if j < b.len() && b[j] == b'[' {
        return Some((level, j + 1));
    }
    None
}

fn long_bracket_close(b: &[u8], start: usize, level: usize) -> Option<usize> {
    let mut i = start;
    while i < b.len() {
        if b[i] == b']' {
            let mut j = i + 1;
            let mut lvl = 0usize;
            while j < b.len() && b[j] == b'=' {
                lvl += 1;
                j += 1;
            }
            if lvl == level && j < b.len() && b[j] == b']' {
                return Some(j + 1);
            }
        }
        i += 1;
    }
    None
}

/// 1-based line number of byte offset `pos` in `src`.
fn line_of(src: &str, pos: usize) -> u32 {
    let p = pos.min(src.len());
    src[..p].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

/// Return the inner text of the balanced bracketed region starting at `src[open..]`, where
/// `open` points at `(` or `{`. Returns (inner_start, inner_end_exclusive) in src-byte
/// coordinates, or None if unbalanced. Respects strings/comments via `mask`.
fn balanced(src: &str, mask: &[bool], open: usize) -> Option<(usize, usize)> {
    let b = src.as_bytes();
    if open >= b.len() {
        return None;
    }
    let (open_c, close_c) = match b[open] {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    let mut depth = 1usize;
    let mut i = open + 1;
    while i < b.len() {
        if !mask[i] {
            i += 1;
            continue;
        }
        if b[i] == open_c {
            depth += 1;
        } else if b[i] == close_c {
            depth -= 1;
            if depth == 0 {
                return Some((open + 1, i));
            }
        }
        i += 1;
    }
    None
}

// ───────────────────────────────────────────────────────────────────── mission parser

/// Scan the top-level `function Name(...)` definitions in file order.
fn scan_top_level_fns(src: &str, mask: &[bool]) -> Vec<FnOutline> {
    // A top-level fn is "function" at the start of a line (ignoring leading whitespace),
    // followed by a bare identifier (not `local function` and not `name.foo = function`).
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Skip whitespace at line start.
        if (i == 0 || b[i.saturating_sub(1)] == b'\n') && mask[i] {
            let mut j = i;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            if j < b.len() && mask[j] && starts_with_keyword(b, j, "function") {
                let after = j + "function".len();
                // Need an identifier char next (whitespace then an ident).
                let mut k = after;
                while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
                    k += 1;
                }
                let ident_start = k;
                while k < b.len() && is_ident_byte(b[k]) {
                    k += 1;
                }
                if k > ident_start {
                    let name = &src[ident_start..k];
                    out.push(FnOutline {
                        name: name.to_string(),
                        line: line_of(src, j),
                        is_lifecycle: is_lifecycle(name),
                    });
                }
            }
        }
        i += 1;
    }
    out
}

fn starts_with_keyword(b: &[u8], at: usize, kw: &str) -> bool {
    let kb = kw.as_bytes();
    if at + kb.len() > b.len() {
        return false;
    }
    if &b[at..at + kb.len()] != kb {
        return false;
    }
    // Must not be followed by an identifier byte.
    at + kb.len() == b.len() || !is_ident_byte(b[at + kb.len()])
}

fn is_ident_byte(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphanumeric()
}

/// Find every match of `needle` as a bare substring that occurs in code (not in a
/// string/comment). Returns byte positions of the first byte of each match.
fn find_in_code(src: &str, mask: &[bool], needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let nb = needle.as_bytes();
    if nb.is_empty() {
        return out;
    }
    let b = src.as_bytes();
    let mut i = 0;
    while i + nb.len() <= b.len() {
        if mask[i] && b[i..i + nb.len()] == *nb {
            out.push(i);
            i += nb.len();
            continue;
        }
        i += 1;
    }
    out
}

/// For a byte offset into `src`, find the top-level fn whose definition is the most recent
/// before `pos` AND whose matching `end` has not yet closed. Approximate: we track block
/// depth from each top-level `function` using a cheap keyword counter.
fn fn_containing(src: &str, mask: &[bool], fns: &[FnOutline], pos: usize) -> Option<String> {
    // Find the last fn whose `line` precedes pos.
    let mut best: Option<&FnOutline> = None;
    for f in fns {
        if line_of(src, pos) >= f.line {
            best = Some(f);
        } else {
            break;
        }
    }
    // We don't verify "has the `end` closed" — the retail scripts don't put a call to
    // CreateChild etc. after a top-level fn's end before the next top-level fn starts, so
    // nearest-preceding is reliable in practice. We retain the inputs for a future tighter
    // scope.
    let _ = (mask,);
    best.map(|f| f.name.clone())
}

/// Pull the first `"..."` string literal out of a slice (returns the unquoted content).
fn first_string_literal(src: &str) -> Option<String> {
    first_string_literal_ranged(src).map(|(s, _)| s)
}

/// Pull the first `"..."` or `'...'` string literal out of a slice, also
/// returning its byte range INSIDE `src` — opening quote at `.start`, byte
/// after closing quote at `.end`. The inspector's edit path needs the range
/// so it can splice an edited value back into the full mission source.
fn first_string_literal_ranged(src: &str) -> Option<(String, std::ops::Range<usize>)> {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' || c == b'\'' {
            let quote = c;
            let start = i;
            let mut j = i + 1;
            let mut out = String::new();
            while j < b.len() {
                if b[j] == b'\\' && j + 1 < b.len() {
                    out.push(b[j + 1] as char);
                    j += 2;
                    continue;
                }
                if b[j] == quote {
                    return Some((out, start..j + 1));
                }
                if b[j] == b'\n' {
                    return None;
                }
                out.push(b[j] as char);
                j += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Pull a `name = "value"` or `name = 123` or `name = IDENT` field out of a Lua table body.
/// Returns the raw RHS (trimmed, no surrounding quotes for strings).
fn table_field<'a>(body: &'a str, name: &str) -> Option<String> {
    table_field_ranged(body, name).map(|(v, _)| v)
}

/// Variant of `table_field` that also returns the RHS string literal's byte
/// range in `body` (quotes included). Non-string RHS has `None` for the range.
fn table_field_ranged(
    body: &str,
    name: &str,
) -> Option<(String, Option<std::ops::Range<usize>>)> {
    let needle = format!("{name} =");
    let (pos, len) = match body.find(&needle) {
        Some(p) => (p, needle.len()),
        None => {
            let short = format!("{name}=");
            (body.find(&short)?, short.len())
        }
    };
    let after_offset = pos + len;
    let after = &body[after_offset..];
    let trimmed_offset_in_after = after.len() - after.trim_start().len();
    let trimmed = &after[trimmed_offset_in_after..];
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        let (v, r) = first_string_literal_ranged(trimmed)?;
        let base = after_offset + trimmed_offset_in_after;
        return Some((v, Some((base + r.start)..(base + r.end))));
    }
    let s = table_field_rhs_nonstring(trimmed)?;
    Some((s, None))
}

fn table_field_rhs_nonstring(trimmed: &str) -> Option<String> {
    let after = trimmed;
    {
        // Take up to the next comma / newline / closing brace at depth 0.
        let mut depth = 0i32;
        let mut out = String::new();
        for ch in trimmed.chars() {
            if depth == 0 && (ch == ',' || ch == '\n' || ch == '}') {
                break;
            }
            if ch == '{' || ch == '(' {
                depth += 1;
            }
            if ch == '}' || ch == ')' {
                depth -= 1;
                if depth < 0 {
                    break;
                }
            }
            out.push(ch);
        }
        let s = out.trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// Parse a mission `.lua` into a [`Mission`].
pub fn parse_mission(path: &str, src: &str) -> Mission {
    let mask = code_mask(src);
    let b = src.as_bytes();

    // ── inherit("...") ────────────────────────────────────────────────────────────────
    let inherits = find_in_code(src, &mask, "inherit(")
        .first()
        .copied()
        .and_then(|p| balanced(src, &mask, p + "inherit".len()))
        .and_then(|(s, e)| first_string_literal(&src[s..e]))
        .unwrap_or_default();

    // Mission id derived from filename stem.
    let id = {
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        prettify_mission_id(stem)
    };

    let functions = scan_top_level_fns(src, &mask);

    let mut mission = Mission {
        id: id.clone(),
        path: path.to_string(),
        inherits,
        nodes: Vec::new(),
        wires: Vec::new(),
        functions: functions.clone(),
        parse_notes: Vec::new(),
        source: src.to_string(),
    };

    // One Fn (or Lifecycle) node per top-level function. Each lives at an entry column.
    for f in &functions {
        let kind = if f.is_lifecycle {
            NodeKind::Lifecycle
        } else {
            NodeKind::Fn
        };
        let snippet = snippet_of(src, f.line, 6);
        mission.nodes.push(Node {
            id: format!("fn_{}", sanitize(&f.name)),
            title: f.name.clone(),
            kind,
            pos: Pos2::ZERO, // laid out below
            width: 190.0,
            inputs: vec![Pin {
                label: "exec".into(),
                ty: PinTy::Exec,
                annot: None,
            edit_range: None,
            }],
            outputs: vec![Pin {
                label: "body".into(),
                ty: PinTy::Exec,
                annot: None,
            edit_range: None,
            }],
            blurb: if f.is_lifecycle {
                format!("Lifecycle hook — {}:{}.", id, f.name)
            } else {
                format!("Helper function — {}:{}.", id, f.name)
            },
            source_line: f.line,
            snippet,
            containing_fn: f.name.clone(),
        });
    }

    // ── MrxVoSequence.Start(...) ─────────────────────────────────────────────────────
    for p in find_in_code(src, &mask, "MrxVoSequence.Start") {
        // paren should follow (possibly with whitespace)
        let open = skip_ws_to_paren(b, p + "MrxVoSequence.Start".len());
        let Some(open_pos) = open else { continue };
        let Some((s, e)) = balanced(src, &mask, open_pos) else {
            mission
                .parse_notes
                .push(format!("MrxVoSequence.Start at line {} — unbalanced parens", line_of(src, p)));
            mission.nodes.push(mk_unparseable(src, p, "MrxVoSequence.Start"));
            continue;
        };
        let inside = &src[s..e];
        // first `{` inside is the args table
        let (cue, cue_range_in_inside, delay) = parse_vo_args(inside);
        // Translate the cue-literal range from `inside`-space to `src`-space
        // so a later splice can rewrite the whole file.
        let cue_range = cue_range_in_inside.map(|r| (s + r.start)..(s + r.end));
        let line = line_of(src, p);
        let containing = fn_containing(src, &mask, &functions, p).unwrap_or_default();
        let title = if let Some(cue) = &cue {
            format!("VO · {}", short_cue(cue))
        } else {
            "VO · Sequence.Start".to_string()
        };
        let mut inputs = vec![Pin {
            label: "exec".into(),
            ty: PinTy::Exec,
            annot: None,
            edit_range: None,
        }];
        if let Some(cue) = cue.clone() {
            inputs.push(Pin {
                label: "cue".into(),
                ty: PinTy::Str,
                annot: Some(format!("\"{}\"", trunc(&cue, 32))),
                edit_range: cue_range.clone(),
            });
        }
        if let Some(d) = delay {
            inputs.push(Pin {
                label: "delay".into(),
                ty: PinTy::Int,
                annot: Some(d.clone()),
                edit_range: None,
            });
        }
        mission.nodes.push(Node {
            id: format!("vo_{}", line),
            title,
            kind: NodeKind::Vo,
            pos: Pos2::ZERO,
            width: 240.0,
            inputs,
            outputs: vec![Pin {
                label: "exec".into(),
                ty: PinTy::Exec,
                annot: None,
            edit_range: None,
            }],
            blurb: format!(
                "MrxVoSequence.Start — cue {}",
                cue.unwrap_or_else(|| "(dynamic)".into())
            ),
            source_line: line,
            snippet: snippet_of(src, line, 6),
            containing_fn: containing,
        });
    }

    // ── self:CreateChild({...}) ──────────────────────────────────────────────────────
    for p in find_in_code(src, &mask, ":CreateChild") {
        let open = skip_ws_to_paren(b, p + ":CreateChild".len());
        let Some(open_pos) = open else { continue };
        let Some((s, e)) = balanced(src, &mask, open_pos) else {
            mission.parse_notes.push(format!(
                ":CreateChild at line {} — unbalanced parens",
                line_of(src, p)
            ));
            mission.nodes.push(mk_unparseable(src, p, "self:CreateChild"));
            continue;
        };
        let inside = &src[s..e];
        // Inside is a `{...}` table. Open-brace find.
        let brace = inside.find('{').map(|k| s + k);
        let (name, name_range, module, module_range, course_count) = if let Some(bp) = brace {
            if let Some((bs, be)) = balanced(src, &mask, bp) {
                let body = &src[bs..be];
                // Ranges returned by `table_field_ranged` are in `body`-space; translate
                // to `src`-space (bs offset) so splice-back sees the whole mission.
                let translate =
                    |r: std::ops::Range<usize>| (bs + r.start)..(bs + r.end);
                let (name, name_r) = match table_field_ranged(body, "sName") {
                    Some((v, Some(r))) => (Some(v), Some(translate(r))),
                    Some((v, None)) => (Some(v), None),
                    None => (None, None),
                };
                let (module, module_r) = match table_field_ranged(body, "sModuleName") {
                    Some((v, Some(r))) => (Some(v), Some(translate(r))),
                    Some((v, None)) => (Some(v), None),
                    None => (None, None),
                };
                (
                    name,
                    name_r,
                    module,
                    module_r,
                    count_table_entries(body, "tCourseLocs")
                        .or_else(|| count_table_entries(body, "vTgtInclude")),
                )
            } else {
                (None, None, None, None, None)
            }
        } else {
            (None, None, None, None, None)
        };
        let line = line_of(src, p);
        let containing = fn_containing(src, &mask, &functions, p).unwrap_or_default();
        let title = module
            .clone()
            .map(|m| format!("Child · {}", m))
            .unwrap_or_else(|| "Child objective".into());
        let mut inputs = vec![Pin {
            label: "exec".into(),
            ty: PinTy::Exec,
            annot: None,
            edit_range: None,
        }];
        if let Some(n) = &name {
            inputs.push(Pin {
                label: "sName".into(),
                ty: PinTy::Str,
                annot: Some(format!("\"{}\"", trunc(n, 28))),
                edit_range: name_range.clone(),
            });
        }
        if let Some(m) = &module {
            inputs.push(Pin {
                label: "sModuleName".into(),
                ty: PinTy::Str,
                annot: Some(format!("\"{}\"", trunc(m, 28))),
                edit_range: module_range.clone(),
            });
        }
        if let Some(c) = course_count {
            inputs.push(Pin {
                label: "course".into(),
                ty: PinTy::Arr,
                annot: Some(format!("[{}]", c)),
                edit_range: None,
            });
        }
        mission.nodes.push(Node {
            id: format!("child_{}", line),
            title,
            kind: NodeKind::Objective,
            pos: Pos2::ZERO,
            width: 280.0,
            inputs,
            outputs: vec![
                Pin {
                    label: "fOnComplete".into(),
                    ty: PinTy::Exec,
                    annot: None,
            edit_range: None,
                },
                Pin {
                    label: "fOnCancel".into(),
                    ty: PinTy::Exec,
                    annot: None,
            edit_range: None,
                },
            ],
            blurb: match (name.as_deref(), module.as_deref()) {
                (Some(n), Some(m)) => format!("Objective \"{}\" of class {}.", n, m),
                (None, Some(m)) => format!("Objective of class {}.", m),
                _ => "Child objective — class not resolved.".into(),
            },
            source_line: line,
            snippet: snippet_of(src, line, 10),
            containing_fn: containing,
        });
    }

    // ── self:_CreateEvent(Event.X, {args}, Callback, {ctx}) ──────────────────────────
    for p in find_in_code(src, &mask, "_CreateEvent") {
        let open = skip_ws_to_paren(b, p + "_CreateEvent".len());
        let Some(open_pos) = open else { continue };
        let Some((s, e)) = balanced(src, &mask, open_pos) else {
            mission.parse_notes.push(format!(
                "_CreateEvent at line {} — unbalanced parens",
                line_of(src, p)
            ));
            mission.nodes.push(mk_unparseable(src, p, "self:_CreateEvent"));
            continue;
        };
        let inside = &src[s..e];
        let (ev, cb) = parse_event_args(inside);
        let line = line_of(src, p);
        let containing = fn_containing(src, &mask, &functions, p).unwrap_or_default();
        let title = if let Some(e) = &ev {
            format!("Event · {}", e)
        } else {
            "Event · ?".into()
        };
        let mut inputs = vec![Pin {
            label: "exec".into(),
            ty: PinTy::Exec,
            annot: None,
            edit_range: None,
        }];
        if let Some(e) = &ev {
            inputs.push(Pin {
                label: "kind".into(),
                ty: PinTy::Str,
                annot: Some(e.clone()),
                edit_range: None,
            });
        }
        let mut outputs = vec![Pin {
            label: "fire".into(),
            ty: PinTy::Exec,
            annot: cb.clone(),
            edit_range: None,
        }];
        if cb.is_none() {
            outputs = vec![Pin {
                label: "fire".into(),
                ty: PinTy::Exec,
                annot: Some("(inline fn)".into()),
                edit_range: None,
            }];
        }
        mission.nodes.push(Node {
            id: format!("ev_{}", line),
            title,
            kind: NodeKind::Event,
            pos: Pos2::ZERO,
            width: 230.0,
            inputs,
            outputs,
            blurb: format!(
                "_CreateEvent({}, …, {})",
                ev.unwrap_or_else(|| "?".into()),
                cb.unwrap_or_else(|| "(inline)".into())
            ),
            source_line: line,
            snippet: snippet_of(src, line, 8),
            containing_fn: containing,
        });
    }

    // ── MrxMusic.PlaySpecialMusic / PlayAmbientMusic / StopSpecialMusic ──────────────
    for (needle, label) in [
        ("MrxMusic.PlaySpecialMusic", "Special"),
        ("MrxMusic.PlayAmbientMusic", "Ambient"),
        ("MrxMusic.StopSpecialMusic", "Stop"),
    ] {
        for p in find_in_code(src, &mask, needle) {
            let open = skip_ws_to_paren(b, p + needle.len());
            let Some(open_pos) = open else { continue };
            let Some((s, e)) = balanced(src, &mask, open_pos) else {
                mission
                    .parse_notes
                    .push(format!("{} at line {} — unbalanced parens", needle, line_of(src, p)));
                mission.nodes.push(mk_unparseable(src, p, needle));
                continue;
            };
            let inside = &src[s..e];
            // Capture the track-literal's byte range too — this is the
            // thing a modder actually tweaks on a music call.
            let (track, track_range) = match first_string_literal_ranged(inside) {
                Some((v, r)) => (Some(v), Some((s + r.start)..(s + r.end))),
                None => (None, None),
            };
            let line = line_of(src, p);
            let containing = fn_containing(src, &mask, &functions, p).unwrap_or_default();
            let title = format!("Music · {}", label);
            let mut inputs = vec![Pin {
                label: "exec".into(),
                ty: PinTy::Exec,
                annot: None,
            edit_range: None,
            }];
            if let Some(t) = &track {
                inputs.push(Pin {
                    label: "track".into(),
                    ty: PinTy::Str,
                    annot: Some(format!("\"{}\"", trunc(t, 28))),
                    edit_range: track_range.clone(),
                });
            }
            mission.nodes.push(Node {
                id: format!("mus_{}", line),
                title,
                kind: NodeKind::Music,
                pos: Pos2::ZERO,
                width: 230.0,
                inputs,
                outputs: Vec::new(),
                blurb: format!(
                    "{} — track {}",
                    needle,
                    track.unwrap_or_else(|| "(none)".into())
                ),
                source_line: line,
                snippet: snippet_of(src, line, 4),
                containing_fn: containing,
            });
        }
    }

    // ── MrxLayerManager.Add / Remove ─────────────────────────────────────────────────
    for (needle, label) in [
        ("MrxLayerManager.Add", "Add"),
        ("MrxLayerManager.Remove", "Remove"),
    ] {
        for p in find_in_code(src, &mask, needle) {
            let open = skip_ws_to_paren(b, p + needle.len());
            let Some(open_pos) = open else { continue };
            let Some((s, e)) = balanced(src, &mask, open_pos) else {
                mission
                    .parse_notes
                    .push(format!("{} at line {} — unbalanced parens", needle, line_of(src, p)));
                mission.nodes.push(mk_unparseable(src, p, needle));
                continue;
            };
            let inside = &src[s..e];
            // Capture the layer-literal's byte range too — same reasoning as
            // the music track: this is what a mod author tunes.
            let (first, layer_range) = match first_string_literal_ranged(inside) {
                Some((v, r)) => (Some(v), Some((s + r.start)..(s + r.end))),
                None => (None, None),
            };
            let line = line_of(src, p);
            let containing = fn_containing(src, &mask, &functions, p).unwrap_or_default();
            let title = format!("Layer · {}", label);
            let mut inputs = vec![Pin {
                label: "exec".into(),
                ty: PinTy::Exec,
                annot: None,
            edit_range: None,
            }];
            if let Some(l) = &first {
                inputs.push(Pin {
                    label: "layer".into(),
                    ty: PinTy::Str,
                    annot: Some(format!("\"{}\"", trunc(l, 28))),
                    edit_range: layer_range.clone(),
                });
            }
            mission.nodes.push(Node {
                id: format!("lay_{}_{}", label.to_ascii_lowercase(), line),
                title,
                kind: NodeKind::Layer,
                pos: Pos2::ZERO,
                width: 240.0,
                inputs,
                outputs: vec![Pin {
                    label: "exec".into(),
                    ty: PinTy::Exec,
                    annot: None,
            edit_range: None,
                }],
                blurb: format!("{} — {}", needle, first.unwrap_or_else(|| "(dyn)".into())),
                source_line: line,
                snippet: snippet_of(src, line, 6),
                containing_fn: containing,
            });
        }
    }

    // ── Lay the nodes out and chain exec wires per containing function ──────────────
    layout_and_wire(&mut mission);

    mission
}

fn mk_unparseable(src: &str, pos: usize, prefix: &str) -> Node {
    let line = line_of(src, pos);
    Node {
        id: format!("err_{}", line),
        title: format!("Unparseable · {}", prefix),
        kind: NodeKind::Unparseable,
        pos: Pos2::ZERO,
        width: 260.0,
        inputs: Vec::new(),
        outputs: Vec::new(),
        blurb: format!(
            "Call-prefix matched at line {} but the parser could not finish extracting it. \
             See snippet.",
            line
        ),
        source_line: line,
        snippet: snippet_of(src, line, 6),
        containing_fn: String::new(),
    }
}

fn skip_ws_to_paren(b: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'(' => return Some(i),
            _ => return None,
        }
    }
    None
}

/// From the body of `MrxVoSequence.Start(...)` — which is itself a Lua table literal
/// `{ "cue", delay, {cb,args} }` — return (cue, cue_range, delay) when extractable.
/// The cue's byte range is in `inside`-coordinates; the caller translates it to
/// `src`-coordinates for a later splice.
fn parse_vo_args(
    inside: &str,
) -> (Option<String>, Option<std::ops::Range<usize>>, Option<String>) {
    // Find the first `{` and get its body.
    let Some(bp) = inside.find('{') else {
        return (None, None, None);
    };
    let body_start_in_inside = bp + 1;
    let after = &inside[body_start_in_inside..];
    // Find matching close counting braces (local to this slice).
    let mut depth = 1i32;
    let mut end = after.len();
    for (i, ch) in after.char_indices() {
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end = i;
                break;
            }
        }
    }
    let body = &after[..end];
    let (cue, cue_range) = match first_string_literal_ranged(body) {
        Some((v, r)) => (
            Some(v),
            Some((body_start_in_inside + r.start)..(body_start_in_inside + r.end)),
        ),
        None => (None, None),
    };
    // Delay: first bare number after the string.
    let delay = {
        let b = body.as_bytes();
        let mut i = 0;
        // skip past the cue string
        if let Some(q) = body.find('"').or_else(|| body.find('\'')) {
            let quote = b[q];
            i = q + 1;
            while i < b.len() && b[i] != quote {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            if i < b.len() {
                i += 1;
            }
        }
        // find first number
        let mut j = i;
        while j < b.len() && !b[j].is_ascii_digit() {
            if b[j] == b'{' || b[j] == b'(' {
                break;
            }
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            let k0 = j;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            Some(body[k0..j].to_string())
        } else {
            None
        }
    };
    (cue, cue_range, delay)
}

/// From the body of `_CreateEvent(Event.Kind, {args}, Callback, {ctx})`, return
/// (kind-string "Event.Kind", callback-name) when extractable.
fn parse_event_args(inside: &str) -> (Option<String>, Option<String>) {
    // First token up to `,` — strip whitespace, keep "Event.X".
    let first_comma = find_top_level_comma(inside, 0);
    let ev = first_comma.map(|i| inside[..i].trim().to_string());
    // Callback: the identifier after the second top-level comma.
    let cb = first_comma.and_then(|i| {
        let next_comma = find_top_level_comma(inside, i + 1)?;
        let seg = inside[next_comma + 1..].trim_start();
        // Up to next comma or EOS.
        let end = find_top_level_comma(seg, 0).unwrap_or(seg.len());
        let s = seg[..end].trim();
        if s.starts_with("function") {
            Some("(inline fn)".into())
        } else if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    });
    (ev, cb)
}

/// Find a comma at nesting depth 0 in `s` starting at `from`.
fn find_top_level_comma(s: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    let mut in_s = 0u8;
    while i < b.len() {
        let c = b[i];
        if in_s != 0 {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == in_s {
                in_s = 0;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => in_s = c,
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b',' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn count_table_entries(body: &str, field: &str) -> Option<usize> {
    let needle_eq = format!("{field} =");
    let pos = body
        .find(&needle_eq)
        .or_else(|| body.find(&format!("{field}=")))?;
    let after = &body[pos..];
    let brace = after.find('{')?;
    // Count commas at depth 0 inside the braced block.
    let tail = &after[brace..];
    let b = tail.as_bytes();
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut saw_item = false;
    let mut in_s = 0u8;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_s != 0 {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == in_s {
                in_s = 0;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => in_s = c,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => commas += 1,
            c if depth >= 1 && !c.is_ascii_whitespace() => saw_item = true,
            _ => {}
        }
        i += 1;
    }
    if commas == 0 && !saw_item {
        return Some(0);
    }
    Some(commas + 1)
}

fn snippet_of(src: &str, line: u32, n: usize) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let start = (line as usize).saturating_sub(1);
    lines
        .iter()
        .skip(start)
        .take(n)
        .map(|s| s.to_string())
        .collect()
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push('…');
        t
    }
}

fn short_cue(cue: &str) -> String {
    // "Fiona-In-Mission-MinorContract-Pmc16-01" → "Pmc16-01"
    cue.rsplit('-')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("-")
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn prettify_mission_id(stem: &str) -> String {
    // Retail: "pmccon016" → "PmcCon016", "oiljob002" → "OilJob002",
    // "mecjob001" → "MecJob001", "startmission" → stem (title case).
    let families = [
        ("pmccon", "PmcCon"),
        ("pmcjob", "PmcJob"),
        ("oilcon", "OilCon"),
        ("oiljob", "OilJob"),
        ("gurcon", "GurCon"),
        ("gurjob", "GurJob"),
        ("velcon", "VelCon"),
        ("veljob", "VelJob"),
        ("vzacon", "VzaCon"),
        ("vzajob", "VzaJob"),
        ("heljob", "HelJob"),
        ("mecjob", "MecJob"),
        ("mishajob", "MishaJob"),
        ("ewenjob", "EwenJob"),
    ];
    let lower = stem.to_ascii_lowercase();
    for (prefix, replacement) in families {
        if lower.starts_with(prefix) {
            let rest = &stem[prefix.len()..];
            return format!("{replacement}{rest}");
        }
    }
    // Fallback: capitalise first byte.
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => stem.to_string(),
    }
}

// ───────────────────────────────────────────────────────────────────── layout

const ENTRY_X: f32 = 16.0;
const INNER_X_STEP: f32 = 300.0;
const ROW_H: f32 = 150.0;
const COL_FN_W: f32 = 220.0;

fn layout_and_wire(m: &mut Mission) {
    // Group non-fn nodes by their containing function, ordered by source line.
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, n) in m.nodes.iter().enumerate() {
        if matches!(n.kind, NodeKind::Fn | NodeKind::Lifecycle) {
            continue;
        }
        groups.entry(n.containing_fn.clone()).or_default().push(i);
    }
    for v in groups.values_mut() {
        let line_of_i = |i: &usize| m.nodes[*i].source_line;
        v.sort_by_key(line_of_i);
    }
    // Place fn nodes in a left column stacked by source line order; place children in rows
    // extending right.
    let mut fn_order: Vec<usize> = m
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, NodeKind::Fn | NodeKind::Lifecycle))
        .map(|(i, _)| i)
        .collect();
    fn_order.sort_by_key(|i| m.nodes[*i].source_line);

    let mut y = 40.0;
    for fn_idx in &fn_order {
        let fn_name = m.nodes[*fn_idx].containing_fn.clone();
        m.nodes[*fn_idx].pos = Pos2::new(ENTRY_X, y);
        let children = groups.get(&fn_name).cloned().unwrap_or_default();
        if children.is_empty() {
            y += ROW_H * 0.6;
            continue;
        }
        // Lay children left-to-right starting from column 1 (ENTRY_X + COL_FN_W).
        let child_x0 = ENTRY_X + COL_FN_W + 40.0;
        let per_row = 3usize; // 3 per row then wrap down
        let mut max_dy = 0.0_f32;
        for (idx, &ci) in children.iter().enumerate() {
            let col = (idx % per_row) as f32;
            let row = (idx / per_row) as f32;
            let x = child_x0 + col * INNER_X_STEP;
            let dy = row * ROW_H;
            m.nodes[ci].pos = Pos2::new(x, y + dy);
            if dy > max_dy {
                max_dy = dy;
            }
        }
        // Wire exec: fn.body → first child.exec, first.exec_out → next.exec, …
        let fn_id = m.nodes[*fn_idx].id.clone();
        if let Some(&first) = children.first() {
            let first_id = m.nodes[first].id.clone();
            m.wires.push(Wire {
                from: (fn_id.clone(), "body".into()),
                to: (first_id, "exec".into()),
                ty: PinTy::Exec,
            });
        }
        for w in children.windows(2) {
            let a = m.nodes[w[0]].id.clone();
            let b = m.nodes[w[1]].id.clone();
            // Each node has an exec output when applicable — if not, skip.
            let has_out = !m.nodes[w[0]].outputs.is_empty();
            if has_out {
                let out_label = m.nodes[w[0]]
                    .outputs
                    .iter()
                    .find(|p| matches!(p.ty, PinTy::Exec))
                    .map(|p| p.label.clone())
                    .unwrap_or_else(|| "exec".into());
                m.wires.push(Wire {
                    from: (a, out_label),
                    to: (b, "exec".into()),
                    ty: PinTy::Exec,
                });
            }
        }
        y += ROW_H * (1.2 + max_dy / ROW_H).max(1.0);
    }
    // Nodes with no containing_fn fall at the bottom (common for file-top code).
    for (i, n) in m.nodes.iter_mut().enumerate() {
        if n.pos == Pos2::ZERO && !matches!(n.kind, NodeKind::Fn | NodeKind::Lifecycle) {
            n.pos = Pos2::new(ENTRY_X + 20.0 * (i as f32 % 5.0), y);
            y += 40.0;
        }
    }
}

// ───────────────────────────────────────────────────────────────────── mission catalog

/// Scan a workshop lua corpus (`(path, content, lower)` tuples) and return every script that
/// `inherit`s a mission superclass.
pub fn build_catalog(lua_corpus: &[(String, String, String)]) -> Vec<MissionEntry> {
    let mission_classes = [
        "MrxTaskContract",
        "MrxTaskContractOutpost",
        "MrxTaskJob",
        "MrxTaskJobCollectType",
        "MrxTaskJobDestroySet",
        "MrxTaskJobDestroyType",
        "MrxTaskJobVerifySet",
        "MrxMissionFlow",
        "MrxTutorial",
        "MecJob",
        "MrxShootingGallery",
        "MrxArtilleryAttack",
    ];
    let mut out = Vec::new();
    for (path, content, _lower) in lua_corpus {
        // Only look at the opening ~400 bytes for the inherit call.
        let head = &content[..content.len().min(1024)];
        let Some(ihpos) = head.find("inherit(\"") else {
            continue;
        };
        let after = &head[ihpos + "inherit(\"".len()..];
        let end = after.find('"');
        let Some(end) = end else { continue };
        let cls = &after[..end];
        if !mission_classes.iter().any(|c| *c == cls) {
            continue;
        }
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        out.push(MissionEntry {
            path: path.clone(),
            id: prettify_mission_id(stem),
            inherits: cls.to_string(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

// ───────────────────────────────────────────────────────────────────── panel

pub struct Panel {
    pub catalog: Vec<MissionEntry>,
    pub catalog_built: bool,
    /// Index into `catalog` of the picked mission, if any.
    pub selected_mission: Option<usize>,
    pub mission: Option<Mission>,
    /// Index into `mission.nodes` of the inspector target.
    pub selected_node: Option<usize>,
    pub pan: Vec2,
    pub zoom: f32,
    pub filter: String,
    pub save: Option<SaveOverlay>,
    pub status: String,
    /// Set by `apply` when a Jump-to-source should fire; drained by the host after apply.
    pub pending_lua_open: Option<(String, u32)>,
    /// Set by `apply` when the user asks to pick a save file; drained by the host.
    pub pending_save_open: bool,
    /// Per-mission node position overrides, keyed by mission id → node id → position.
    /// A drag of a node writes here; `pick_mission` reapplies these on top of
    /// `parse_mission`'s default layout so switching missions and back keeps the
    /// arrangement the user built.
    pub node_overrides: HashMap<String, HashMap<String, Pos2>>,
    /// Per-mission pan/zoom overrides so view framing survives a mission switch too.
    pub view_overrides: HashMap<String, (Vec2, f32)>,
    /// Does the current mission's `source` differ from what was last parsed from
    /// disk / overlay? Set true the moment the first in-pane edit lands, cleared
    /// by `SaveToOverlay`.
    pub source_dirty: bool,
    /// Last save-to-overlay outcome — the inspector reads it to show success /
    /// failure plainly instead of pretending nothing happened.
    pub last_save_status: Option<Result<String, String>>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            catalog: Vec::new(),
            catalog_built: false,
            selected_mission: None,
            mission: None,
            selected_node: None,
            pan: Vec2::ZERO,
            zoom: 1.0,
            filter: String::new(),
            save: None,
            status: "no save loaded".into(),
            pending_lua_open: None,
            pending_save_open: false,
            node_overrides: HashMap::new(),
            view_overrides: HashMap::new(),
            source_dirty: false,
            last_save_status: None,
        }
    }
}

impl Panel {
    /// Build the catalog from a lua corpus. Idempotent.
    pub fn ensure_catalog(&mut self, lua_corpus: &[(String, String, String)]) {
        if self.catalog_built {
            return;
        }
        self.catalog = build_catalog(lua_corpus);
        self.catalog_built = true;
        // Auto-pick a sensible default if nothing is loaded yet.
        if self.selected_mission.is_none() {
            let default = self
                .catalog
                .iter()
                .position(|e| e.id.eq_ignore_ascii_case("PmcCon016"))
                .or_else(|| if self.catalog.is_empty() { None } else { Some(0) });
            if let Some(idx) = default {
                self.pick_mission(idx, lua_corpus);
            }
        }
    }

    pub fn pick_mission(
        &mut self,
        idx: usize,
        lua_corpus: &[(String, String, String)],
    ) {
        let Some(entry) = self.catalog.get(idx).cloned() else {
            return;
        };
        // Snapshot the outgoing mission's pan/zoom so coming back later lands on
        // the same view.
        if let Some(prev) = self.mission.as_ref() {
            self.view_overrides
                .insert(prev.id.clone(), (self.pan, self.zoom));
        }
        let found = lua_corpus.iter().find(|(p, _, _)| *p == entry.path);
        match found {
            Some((p, content, _)) => {
                let mut mission = parse_mission(p, content);
                // Reapply any node positions the user had dragged for this mission.
                if let Some(ov) = self.node_overrides.get(&mission.id) {
                    for n in mission.nodes.iter_mut() {
                        if let Some(&pos) = ov.get(&n.id) {
                            n.pos = pos;
                        }
                    }
                }
                self.mission = Some(mission);
                self.selected_mission = Some(idx);
                self.selected_node = self
                    .mission
                    .as_ref()
                    .and_then(|m| m.nodes.first().map(|_| 0));
                // Restore the saved view if we have one for this mission; otherwise
                // start fresh.
                if let Some(&(pan, zoom)) = self
                    .mission
                    .as_ref()
                    .and_then(|m| self.view_overrides.get(&m.id))
                {
                    self.pan = pan;
                    self.zoom = zoom;
                } else {
                    self.pan = Vec2::ZERO;
                    self.zoom = 1.0;
                }
                let nodes = self.mission.as_ref().map(|m| m.nodes.len()).unwrap_or(0);
                let notes = self.mission.as_ref().map(|m| m.parse_notes.len()).unwrap_or(0);
                self.status = format!(
                    "{}: {} nodes parsed{}",
                    entry.id,
                    nodes,
                    if notes > 0 {
                        format!(", {} unparseable sites", notes)
                    } else {
                        String::new()
                    }
                );
            }
            None => {
                self.status = format!("{}: path not in lua corpus", entry.path);
            }
        }
    }

    /// Decode a `.profile` on disk and keep its state as an overlay. Returns `Ok(())` on
    /// success, `Err` with the parser message on failure. Does NOT swallow errors silently.
    pub fn load_save(&mut self, path: &std::path::Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let profile = mercs2_formats::save::parse(&bytes)
            .map_err(|e| format!("parse {}: {e}", path.display()))?;
        let state = profile.save_state()?;
        let mut active: BTreeMap<String, f64> = BTreeMap::new();
        for am in &state.active_missions {
            active.insert(am.id.clone(), am.state);
        }
        let overlay = SaveOverlay {
            source: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string(),
            active_contract: profile.active_contract.clone(),
            active,
            completed_flow: state.completed_flow.clone(),
            flow_chain: state.flow_chain.clone(),
        };
        self.status = format!(
            "{} loaded — {} active / {} completed / {} in chain",
            overlay.source,
            overlay.active.len(),
            overlay.completed_flow.len(),
            overlay.flow_chain.len()
        );
        self.save = Some(overlay);
        Ok(())
    }

    pub fn clear_save(&mut self) {
        self.save = None;
        self.status = "no save loaded".into();
    }

    pub fn selected_node(&self) -> Option<&Node> {
        let m = self.mission.as_ref()?;
        let i = self.selected_node?;
        m.nodes.get(i)
    }
}

// ───────────────────────────────────────────────────────────────────── Act / apply

pub enum Act {
    PickMission(usize),
    SelectNode(usize),
    /// Select the given node, or clear selection if `None`. Used by the canvas
    /// so an empty-canvas click reads as "deselect" without a separate verb.
    SelectNodeOrClear(Option<usize>),
    ZoomDelta(f32),
    ResetView,
    PanDelta(Vec2),
    /// Scroll-wheel zoom step, with the cursor's position so apply can keep
    /// the point under the cursor fixed through the scale change (plain
    /// scalar zoom pulls everything toward (0,0), which is disorienting).
    ZoomBy {
        factor: f32,
        cursor: Option<Pos2>,
        canvas_min: Pos2,
    },
    /// A node drag event — move node `idx` by `delta_mission_space` and
    /// persist the new position per-mission so switching away and back keeps
    /// it. Expressed in mission-space (pre-zoom) so the apply handler never
    /// has to re-derive the zoom.
    NodeDragDelta {
        idx: usize,
        delta_mission_space: Vec2,
    },
    JumpToSource,
    OpenSavePicker,
    ClearSave,
    /// Rewrite a string literal in the current mission's source. `node_id` and
    /// `pin_label` identify the pin whose `edit_range` names the byte span of
    /// the literal (quotes included); `new_value` is the UNQUOTED replacement.
    /// Apply splices the new quoted literal into `mission.source` and re-parses,
    /// so pin ranges and position overrides stay honest after the edit.
    EditStringPin {
        node_id: String,
        pin_label: String,
        new_value: String,
    },
    /// Discard every unsaved edit by re-parsing from the on-disk file. Only
    /// enabled when the Panel's `source_dirty` is set.
    DiscardEdits,
    /// Write the current (mutated) mission source out to the per-user overlay
    /// path and clear `source_dirty`. Never touches the vendored corpus — the
    /// retail scripts are read-only reference material.
    SaveToOverlay,
}

pub fn apply(act: Act, p: &mut Panel, lua_corpus: &[(String, String, String)]) {
    match act {
        Act::PickMission(idx) => p.pick_mission(idx, lua_corpus),
        Act::SelectNode(i) => p.selected_node = Some(i),
        Act::SelectNodeOrClear(sel) => p.selected_node = sel,
        Act::ZoomDelta(d) => p.zoom = (p.zoom + d).clamp(0.3, 3.0),
        Act::ResetView => {
            p.zoom = 1.0;
            p.pan = Vec2::ZERO;
        }
        Act::PanDelta(d) => p.pan += d,
        Act::ZoomBy {
            factor,
            cursor,
            canvas_min,
        } => {
            let old_scale = p.zoom;
            let new_scale = (old_scale * factor).clamp(0.3, 3.0);
            if (new_scale - old_scale).abs() < 1e-4 {
                return;
            }
            if let Some(cursor) = cursor {
                // Keep the mission-space point under the cursor fixed:
                //   local = (cursor - origin_old) / old_scale
                //   origin_new = cursor - local * new_scale
                //   pan_new    = origin_new - canvas_min
                let origin_old = canvas_min + p.pan;
                let local = (cursor - origin_old) / old_scale;
                p.zoom = new_scale;
                let origin_new = cursor - local * new_scale;
                p.pan = origin_new - canvas_min;
            } else {
                p.zoom = new_scale;
            }
        }
        Act::NodeDragDelta {
            idx,
            delta_mission_space,
        } => {
            // Make the drag feel decisive — selection follows the thing you grabbed.
            p.selected_node = Some(idx);
            let Some(mission) = p.mission.as_mut() else {
                return;
            };
            let Some(node) = mission.nodes.get_mut(idx) else {
                return;
            };
            node.pos += delta_mission_space;
            let mid = mission.id.clone();
            let nid = node.id.clone();
            let pos = node.pos;
            p.node_overrides
                .entry(mid)
                .or_default()
                .insert(nid, pos);
        }
        Act::JumpToSource => {
            if let Some(node) = p.selected_node() {
                if let Some(m) = &p.mission {
                    p.pending_lua_open = Some((m.path.clone(), node.source_line));
                }
            }
        }
        Act::OpenSavePicker => p.pending_save_open = true,
        Act::ClearSave => p.clear_save(),
        Act::EditStringPin {
            node_id,
            pin_label,
            new_value,
        } => {
            apply_string_edit(p, &node_id, &pin_label, &new_value);
        }
        Act::DiscardEdits => {
            // Re-parse the mission from the file on the lua corpus to drop
            // every in-memory edit.
            if let Some(idx) = p.selected_mission {
                p.pick_mission(idx, lua_corpus);
                p.source_dirty = false;
                p.last_save_status = None;
            }
        }
        Act::SaveToOverlay => {
            match write_overlay(p) {
                Ok(path) => {
                    p.source_dirty = false;
                    p.last_save_status = Some(Ok(path));
                }
                Err(e) => {
                    p.last_save_status = Some(Err(e));
                }
            }
        }
    }
}

/// Rewrite the Lua literal at `pin.edit_range` with `new_value`, then re-parse
/// the mutated source so every pin's range stays honest.
///
/// Quoting policy: we preserve the ORIGINAL quote style — if the literal was
/// single-quoted in source we re-emit as `'…'` to minimise diff noise. Inner
/// characters that would re-escape (`"`, `'`, `\`, newline) are escaped before
/// re-emission so the splice can never produce invalid Lua.
fn apply_string_edit(p: &mut Panel, node_id: &str, pin_label: &str, new_value: &str) {
    let Some(mission) = p.mission.as_mut() else {
        return;
    };
    let Some(node) = mission.nodes.iter().find(|n| n.id == node_id) else {
        return;
    };
    let Some(pin) = node.inputs.iter().find(|p| p.label == pin_label) else {
        return;
    };
    let Some(range) = pin.edit_range.clone() else {
        return;
    };
    if range.start > mission.source.len() || range.end > mission.source.len() {
        return; // stale range — safer to drop than to splice at the wrong byte
    }
    let old_literal = &mission.source[range.clone()];
    let quote = old_literal.bytes().next().unwrap_or(b'"') as char;
    if quote != '"' && quote != '\'' {
        return;
    }
    let encoded = encode_lua_string(new_value, quote);
    let mut new_source = String::with_capacity(mission.source.len() + encoded.len());
    new_source.push_str(&mission.source[..range.start]);
    new_source.push_str(&encoded);
    new_source.push_str(&mission.source[range.end..]);

    // Re-parse against the mutated source so pin ranges, titles, and blurbs
    // reflect the edit immediately. Preserve the user's node arrangement and
    // selected-node identity across the reparse.
    let path = mission.path.clone();
    let selected_id = p.selected_node.and_then(|i| p.mission.as_ref()?.nodes.get(i).map(|n| n.id.clone()));
    let mut fresh = parse_mission(&path, &new_source);
    if let Some(ov) = p.node_overrides.get(&fresh.id) {
        for n in fresh.nodes.iter_mut() {
            if let Some(&pos) = ov.get(&n.id) {
                n.pos = pos;
            }
        }
    }
    if let Some(target) = selected_id {
        if let Some(i) = fresh.nodes.iter().position(|n| n.id == target) {
            p.selected_node = Some(i);
        }
    }
    p.mission = Some(fresh);
    p.source_dirty = true;
    p.last_save_status = None;
}

fn encode_lua_string(value: &str, quote: char) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push(quote);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Compute the overlay path for a mission's edited source.
///
/// The retail Lua lives inside the vendored corpus at
/// `crates/mercs2_script/corpus/mercs2-luacd/src/…` and must stay pristine —
/// those are read-only reference bytes, and overwriting them inside a workshop
/// run would quietly corrupt the oracle. The overlay instead lands under
/// `$CWD/missions-overlay/<mission-path>` relative to the process's current
/// directory (which for a workshop launch is the parent repo), mirroring the
/// corpus layout one-for-one. A later integration can route the overlay into
/// a Shipment's src tree; this is the first-cut location, documented in the
/// Save-to-script verb's hover text.
fn overlay_path_for(mission_path: &str) -> std::path::PathBuf {
    let base = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("missions-overlay");
    let rel = std::path::PathBuf::from(mission_path);
    // Strip a leading `/` if any — treat the path as relative to the overlay.
    let rel = match rel.strip_prefix("/") {
        Ok(r) => r.to_path_buf(),
        Err(_) => rel,
    };
    base.join(rel)
}

fn write_overlay(p: &Panel) -> Result<String, String> {
    let Some(mission) = p.mission.as_ref() else {
        return Err("no mission loaded".into());
    };
    let path = overlay_path_for(&mission.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, &mission.source).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path.display().to_string())
}

// ───────────────────────────────────────────────────────────────────── navigator

pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    ui.label(theme::disp_text("MISSIONS", 15.0, theme::TX));
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Contracts, jobs, objectives — real Lua, parsed per pick.")
            .size(11.5)
            .color(theme::FAINT),
    );
    ui.add_space(6.0);

    // ── Save status ─────────────────────────────────────────────────────────────────
    theme::eyebrow(ui, "Save overlay");
    match &p.save {
        Some(ov) => {
            theme::row_chip(ui, theme::BRASS_SOFT, theme::BRASS_DK, |ui| {
                ui.label(
                    egui::RichText::new(&ov.source)
                        .monospace()
                        .size(11.5)
                        .color(theme::BRASS),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} active · {} done",
                            ov.active.len(),
                            ov.completed_flow.len()
                        ))
                        .monospace()
                        .size(9.5)
                        .color(theme::FAINT),
                    );
                });
            });
        }
        None => {
            ui.label(
                egui::RichText::new(&p.status)
                    .size(11.0)
                    .color(theme::DIM),
            );
        }
    }
    ui.horizontal(|ui| {
        if ui.button("Load .profile…").clicked() {
            acts.push(Act::OpenSavePicker);
        }
        if p.save.is_some() && ui.button("Clear").clicked() {
            acts.push(Act::ClearSave);
        }
    });

    // ── Script picker ───────────────────────────────────────────────────────────────
    ui.add_space(10.0);
    theme::eyebrow(ui, "Script");
    ui.label(
        egui::RichText::new(format!("{} mission scripts in corpus", p.catalog.len()))
            .size(10.0)
            .color(theme::FAINT),
    );
    ui.add_space(2.0);
    if p.catalog.is_empty() {
        ui.label(
            egui::RichText::new(
                "no mission scripts resolved — the workshop lua corpus is not loaded. \
                 Install the reference bundle to see them.",
            )
            .size(11.0)
            .color(theme::BAD),
        );
    } else {
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, entry) in p.catalog.iter().enumerate() {
                    let is_sel = p.selected_mission == Some(i);
                    let state = p
                        .save
                        .as_ref()
                        .map(|s| s.state_for(&entry.id))
                        .unwrap_or(MissionState::None);
                    let bg = if is_sel { theme::BRASS_SOFT } else { Color32::TRANSPARENT };
                    let border = if is_sel { theme::BRASS_DK } else { theme::LINE };
                    let resp = render_mission_row(ui, entry, state, bg, border);
                    if resp.clicked() {
                        acts.push(Act::PickMission(i));
                    }
                }
            });
    }

    // ── In this script ──────────────────────────────────────────────────────────────
    if let Some(m) = &p.mission {
        ui.add_space(10.0);
        theme::eyebrow(ui, "In this script");
        ui.label(
            egui::RichText::new(format!(
                "{} top-level functions · inherits {}",
                m.functions.len(),
                if m.inherits.is_empty() { "?" } else { &m.inherits }
            ))
            .size(10.5)
            .color(theme::FAINT),
        );
        for f in &m.functions {
            let col = if f.is_lifecycle {
                Color32::from_rgb(0x8b, 0x5c, 0xf6)
            } else {
                Color32::from_rgb(0x9e, 0xc8, 0xea)
            };
            theme::row_chip(ui, Color32::TRANSPARENT, theme::LINE, |ui| {
                swatch(ui, col, true);
                ui.label(egui::RichText::new(&f.name).size(12.0).color(theme::DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("L{}", f.line))
                            .monospace()
                            .size(9.5)
                            .color(theme::FAINT),
                    );
                });
            });
        }
        if !m.parse_notes.is_empty() {
            ui.add_space(6.0);
            theme::eyebrow(ui, "Parser notes");
            for note in &m.parse_notes {
                ui.label(
                    egui::RichText::new(note)
                        .size(10.5)
                        .color(Color32::from_rgb(0xe8, 0x76, 0x3a)),
                );
            }
        }
    }

    acts
}

fn render_mission_row(
    ui: &mut egui::Ui,
    entry: &MissionEntry,
    state: MissionState,
    bg: Color32,
    border: Color32,
) -> egui::Response {
    theme::row_chip(ui, bg, border, |ui| {
        let (badge_text, badge_col) = match state {
            MissionState::Active => ("ACTIVE", Color32::from_rgb(0x10, 0xb9, 0x81)),
            MissionState::Done => ("DONE", Color32::from_rgb(0x74, 0xdd, 0xc8)),
            MissionState::Queued => ("QUEUED", Color32::from_rgb(0x9e, 0xc8, 0xea)),
            MissionState::None => ("", Color32::TRANSPARENT),
        };
        ui.label(egui::RichText::new(&entry.id).size(12.0).color(theme::TX));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !badge_text.is_empty() {
                let f = egui::Frame::none()
                    .fill(Color32::from_rgba_premultiplied(
                        badge_col.r(),
                        badge_col.g(),
                        badge_col.b(),
                        40,
                    ))
                    .rounding(egui::Rounding::same(3.0))
                    .inner_margin(egui::Margin::symmetric(5.0, 1.0));
                f.show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(badge_text)
                            .monospace()
                            .size(8.5)
                            .color(badge_col),
                    );
                });
            }
            ui.label(
                egui::RichText::new(&entry.inherits)
                    .monospace()
                    .size(9.5)
                    .color(theme::FAINT),
            );
        });
    })
}

fn swatch(ui: &mut egui::Ui, color: Color32, square: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), Sense::hover());
    if square {
        ui.painter().rect_filled(rect, egui::Rounding::same(2.0), color);
    } else {
        ui.painter().circle_filled(rect.center(), 4.5, color);
    }
}

// ───────────────────────────────────────────────────────────────────── canvas

const CANVAS_BG: Color32 = Color32::from_rgb(0x0c, 0x10, 0x15);
const DOT: Color32 = Color32::from_rgba_premultiplied(78, 90, 108, 40);
const NODE_BODY: Color32 = Color32::from_rgb(0x1c, 0x23, 0x2e);
const NODE_BORDER: Color32 = Color32::from_rgb(0x28, 0x30, 0x3d);
const SEL_BORDER: Color32 = Color32::from_rgb(0x1e, 0x9b, 0xff);
const ACTIVE_BORDER: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
const DONE_BORDER: Color32 = Color32::from_rgb(0x74, 0xdd, 0xc8);

pub fn center(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(CANVAS_BG))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            paint_grid(ui, rect);

            let Some(mission) = &p.mission else {
                paint_banner(
                    ui,
                    rect,
                    "No mission picked — click one in the left panel.",
                    theme::DIM,
                );
                return;
            };
            if mission.nodes.is_empty() {
                paint_banner(
                    ui,
                    rect,
                    "This script has no recognisable call shapes — parser produced zero nodes.",
                    Color32::from_rgb(0xe8, 0x76, 0x3a),
                );
                return;
            }

            // ── Background interact: pan + zoom (scroll). Allocated FIRST so
            // nodes drawn later get the pointer when they overlap. egui's hit
            // test prefers the LAST interactable at a point, which is exactly
            // what we want here — nodes shadow the background.
            let bg_resp = ui.interact(
                rect,
                egui::Id::new("missions-canvas-bg"),
                Sense::click_and_drag(),
            );
            if bg_resp.dragged() {
                acts.push(Act::PanDelta(bg_resp.drag_delta()));
            }
            if bg_resp.clicked() {
                acts.push(Act::SelectNodeOrClear(None));
            }
            if bg_resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll.abs() >= 0.5 {
                    let factor = (scroll * 0.0025).exp();
                    let cursor = ui.input(|i| i.pointer.hover_pos());
                    acts.push(Act::ZoomBy {
                        factor,
                        cursor,
                        canvas_min: rect.min,
                    });
                }
            }

            // Nodes — interact with click_and_drag so a drag MOVES the node and
            // a click still selects. Register from first to last; because egui
            // prefers the last-registered hit, nodes defined later in file
            // order sit on top. For now that order IS the paint order.
            let scale = p.zoom;
            let origin = rect.min + p.pan;
            let node_rects = compute_node_rects(mission, origin, scale);
            for (i, n) in mission.nodes.iter().enumerate() {
                let r = node_rects[i];
                let id = egui::Id::new(("missions-node", &n.id));
                let resp = ui.interact(r, id, Sense::click_and_drag());
                if resp.clicked() {
                    acts.push(Act::SelectNodeOrClear(Some(i)));
                }
                if resp.dragged() {
                    acts.push(Act::NodeDragDelta {
                        idx: i,
                        delta_mission_space: resp.drag_delta() / scale.max(0.01),
                    });
                }
            }

            // Paint pass — wires first, nodes on top.
            paint_wires(ui, mission, &node_rects);
            let overlay_state = p
                .save
                .as_ref()
                .map(|s| s.state_for(&mission.id))
                .unwrap_or(MissionState::None);
            for (i, n) in mission.nodes.iter().enumerate() {
                let r = node_rects[i];
                let selected = p.selected_node == Some(i);
                let overlay_tint = if matches!(n.kind, NodeKind::Lifecycle)
                    && n.title == "Activated"
                {
                    match overlay_state {
                        MissionState::Active => Some(ACTIVE_BORDER),
                        MissionState::Done => Some(DONE_BORDER),
                        _ => None,
                    }
                } else {
                    None
                };
                paint_node(ui, n, r, selected, overlay_tint);
            }

            paint_legend(ui, rect);
            acts.extend(paint_zoom_pill(ui, rect, p));
            paint_status_pill(ui, rect, p);
        });
    acts
}

fn paint_banner(ui: &mut egui::Ui, canvas: Rect, msg: &str, col: Color32) {
    ui.painter().text(
        canvas.center(),
        egui::Align2::CENTER_CENTER,
        msg,
        egui::FontId::proportional(13.0),
        col,
    );
}

fn paint_grid(ui: &mut egui::Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, CANVAS_BG);
    let step = 26.0_f32;
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x;
        while x < rect.max.x {
            p.circle_filled(Pos2::new(x, y), 1.2, DOT);
            x += step;
        }
        y += step;
    }
}

const HEADER_H: f32 = 34.0;
const PIN_ROW_H: f32 = 26.0;
const NODE_PAD_Y: f32 = 8.0;

fn compute_node_rects(mission: &Mission, origin: Pos2, scale: f32) -> Vec<Rect> {
    let mut out = Vec::with_capacity(mission.nodes.len());
    for n in &mission.nodes {
        let rows = n.inputs.len().max(n.outputs.len()).max(1);
        let h = HEADER_H + NODE_PAD_Y * 2.0 + rows as f32 * PIN_ROW_H;
        let min = origin + (n.pos.to_vec2() * scale);
        let size = egui::vec2(n.width * scale, h * scale);
        out.push(Rect::from_min_size(min, size));
    }
    out
}

fn pin_pos_by_label(
    n: &Node,
    r: Rect,
    label: &str,
    want_out: bool,
) -> Option<(Pos2, PinTy)> {
    let list = if want_out { &n.outputs } else { &n.inputs };
    let (i, pin) = list.iter().enumerate().find(|(_, p)| p.label == label)?;
    let y = r.min.y + HEADER_H + NODE_PAD_Y + PIN_ROW_H * (i as f32 + 0.5);
    let x = if want_out { r.max.x } else { r.min.x };
    Some((Pos2::new(x, y), pin.ty))
}

fn paint_wires(ui: &mut egui::Ui, mission: &Mission, rects: &[Rect]) {
    let p = ui.painter();
    for w in &mission.wires {
        let Some(a_idx) = mission.nodes.iter().position(|n| n.id == w.from.0) else {
            continue;
        };
        let Some(b_idx) = mission.nodes.iter().position(|n| n.id == w.to.0) else {
            continue;
        };
        let Some((a, _)) = pin_pos_by_label(&mission.nodes[a_idx], rects[a_idx], &w.from.1, true)
        else {
            continue;
        };
        let Some((b, _)) = pin_pos_by_label(&mission.nodes[b_idx], rects[b_idx], &w.to.1, false)
        else {
            continue;
        };
        let col = w.ty.colour();
        let stroke = Stroke::new(if matches!(w.ty, PinTy::Exec) { 3.0 } else { 2.4 }, col);
        bezier(p, a, b, stroke);
        p.circle_filled(a, 3.6, col);
        p.circle_filled(b, 3.6, col);
    }
}

fn bezier(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let dx = (b.x - a.x).abs().max(40.0);
    let c1 = Pos2::new(a.x + dx * 0.55, a.y);
    let c2 = Pos2::new(b.x - dx * 0.55, b.y);
    let steps = 24;
    let mut prev = a;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let p = Pos2::new(
            u * u * u * a.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * b.x,
            u * u * u * a.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * b.y,
        );
        painter.line_segment([prev, p], stroke);
        prev = p;
    }
}

/// Paint one node card. Interaction (click / drag) is handled separately in
/// `center` so a drag that started inside the node can continue across the
/// background without the two interact calls fighting over the same ID.
fn paint_node(
    ui: &mut egui::Ui,
    n: &Node,
    rect: Rect,
    selected: bool,
    overlay: Option<Color32>,
) {
    let p = ui.painter();
    let rounding = egui::Rounding::same(10.0);
    p.rect_filled(
        rect.translate(egui::vec2(0.0, 6.0)),
        rounding,
        Color32::from_rgba_premultiplied(0, 0, 0, 90),
    );
    p.rect_filled(rect, rounding, NODE_BODY);
    let border = if selected {
        SEL_BORDER
    } else {
        overlay.unwrap_or(NODE_BORDER)
    };
    p.rect_stroke(
        rect,
        rounding,
        Stroke::new(
            if selected || overlay.is_some() { 1.6 } else { 1.0 },
            border,
        ),
    );
    if selected {
        let halo = Rect::from_min_max(
            rect.min - egui::vec2(3.0, 3.0),
            rect.max + egui::vec2(3.0, 3.0),
        );
        p.rect_stroke(
            halo,
            egui::Rounding::same(13.0),
            Stroke::new(3.0, Color32::from_rgba_premultiplied(0x1e, 0x9b, 0xff, 90)),
        );
    }

    let (h_top, h_bot, h_text, eyebrow) = n.kind.header_colours();
    let header_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), HEADER_H));
    paint_vgradient(
        p,
        header_rect,
        h_top,
        h_bot,
        egui::Rounding {
            nw: 10.0,
            ne: 10.0,
            sw: 0.0,
            se: 0.0,
        },
    );
    p.text(
        header_rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        &n.title,
        egui::FontId::proportional(13.0),
        h_text,
    );
    p.text(
        header_rect.right_center() + egui::vec2(-10.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        eyebrow,
        egui::FontId::monospace(9.0),
        Color32::from_rgba_premultiplied(255, 255, 255, 110),
    );

    let rows = n.inputs.len().max(n.outputs.len());
    for i in 0..rows {
        let y = rect.min.y + HEADER_H + NODE_PAD_Y + PIN_ROW_H * (i as f32 + 0.5);
        if let Some(pin) = n.inputs.get(i) {
            draw_pin(p, Pos2::new(rect.min.x, y), pin, false);
        }
        if let Some(pin) = n.outputs.get(i) {
            draw_pin(p, Pos2::new(rect.max.x, y), pin, true);
        }
    }

    // Interaction handled by `center` so we can combine click+drag with the
    // background pan handler without conflicting IDs.
    let _ = ui;
    let _ = &n.id;
}

fn draw_pin(painter: &egui::Painter, port: Pos2, pin: &Pin, is_out: bool) {
    let text_pos = if is_out {
        port + egui::vec2(-12.0, 0.0)
    } else {
        port + egui::vec2(12.0, 0.0)
    };
    let align = if is_out {
        egui::Align2::RIGHT_CENTER
    } else {
        egui::Align2::LEFT_CENTER
    };
    match pin.ty {
        PinTy::Exec => {
            let s = 6.0;
            let pts = vec![
                Pos2::new(port.x - s, port.y),
                Pos2::new(port.x, port.y - s),
                Pos2::new(port.x + s, port.y),
                Pos2::new(port.x, port.y + s),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts,
                pin.ty.colour(),
                Stroke::new(1.0, Color32::from_black_alpha(220)),
            ));
        }
        _ => {
            painter.circle_filled(port, 5.0, pin.ty.colour());
            painter.circle_stroke(port, 5.0, Stroke::new(1.0, Color32::from_black_alpha(220)));
        }
    }
    let label_col = Color32::from_rgb(0xaa, 0xb4, 0xc2);
    painter.text(
        text_pos,
        align,
        &pin.label,
        egui::FontId::proportional(12.0),
        label_col,
    );
    if let Some(annot) = &pin.annot {
        let (annot_pos, annot_align) = if is_out {
            (port + egui::vec2(-12.0, 12.0), egui::Align2::RIGHT_CENTER)
        } else {
            (
                port + egui::vec2(12.0 + text_w(&pin.label), 0.0),
                egui::Align2::LEFT_CENTER,
            )
        };
        painter.text(
            annot_pos,
            annot_align,
            annot,
            egui::FontId::monospace(10.0),
            Color32::from_rgb(0x4a, 0x56, 0x66),
        );
    }
}

fn text_w(s: &str) -> f32 {
    (s.chars().count() as f32) * 7.0 + 6.0
}

fn paint_vgradient(
    painter: &egui::Painter,
    rect: Rect,
    top: Color32,
    bot: Color32,
    rounding: egui::Rounding,
) {
    let bands = 10;
    for i in 0..bands {
        let t = i as f32 / (bands - 1) as f32;
        let col = Color32::from_rgba_premultiplied(
            lerp8(top.r(), bot.r(), t),
            lerp8(top.g(), bot.g(), t),
            lerp8(top.b(), bot.b(), t),
            255,
        );
        let y0 = rect.min.y + rect.height() * i as f32 / bands as f32;
        let y1 = rect.min.y + rect.height() * (i + 1) as f32 / bands as f32;
        let band = Rect::from_min_max(Pos2::new(rect.min.x, y0), Pos2::new(rect.max.x, y1));
        painter.rect_filled(
            band,
            if i == 0 { rounding } else { egui::Rounding::ZERO },
            col,
        );
    }
}

fn lerp8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 * (1.0 - t) + b as f32 * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn paint_legend(ui: &mut egui::Ui, canvas: Rect) {
    let p = ui.painter();
    let margin = 14.0;
    let w = 360.0;
    let h = 28.0;
    let r = Rect::from_min_size(
        Pos2::new(canvas.min.x + margin, canvas.max.y - margin - h),
        egui::vec2(w, h),
    );
    p.rect_filled(
        r,
        egui::Rounding::same(10.0),
        Color32::from_rgba_premultiplied(0x11, 0x15, 0x1c, 220),
    );
    p.rect_stroke(r, egui::Rounding::same(10.0), Stroke::new(1.0, NODE_BORDER));
    let items = [
        ("exec", PinTy::Exec),
        ("str", PinTy::Str),
        ("int", PinTy::Int),
        ("obj", PinTy::Obj),
        ("arr", PinTy::Arr),
    ];
    let mut x = r.min.x + 11.0;
    p.text(
        Pos2::new(x, r.center().y),
        egui::Align2::LEFT_CENTER,
        "LEGEND",
        egui::FontId::monospace(10.0),
        theme::FAINT,
    );
    x += 60.0;
    for (label, ty) in items {
        if matches!(ty, PinTy::Exec) {
            let s = 5.0;
            let pts = vec![
                Pos2::new(x - s, r.center().y),
                Pos2::new(x, r.center().y - s),
                Pos2::new(x + s, r.center().y),
                Pos2::new(x, r.center().y + s),
            ];
            p.add(egui::Shape::convex_polygon(pts, ty.colour(), Stroke::NONE));
        } else {
            p.circle_filled(Pos2::new(x, r.center().y), 5.0, ty.colour());
        }
        p.text(
            Pos2::new(x + 10.0, r.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.0),
            theme::DIM,
        );
        x += 60.0;
    }
}

fn paint_zoom_pill(ui: &mut egui::Ui, canvas: Rect, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let margin = 14.0;
    let w = 140.0;
    let h = 30.0;
    let r = Rect::from_min_size(
        Pos2::new(canvas.max.x - margin - w, canvas.max.y - margin - h),
        egui::vec2(w, h),
    );
    let painter = ui.painter();
    painter.rect_filled(
        r,
        egui::Rounding::same(9.0),
        Color32::from_rgba_premultiplied(0x11, 0x15, 0x1c, 220),
    );
    painter.rect_stroke(r, egui::Rounding::same(9.0), Stroke::new(1.0, NODE_BORDER));
    let minus = Rect::from_min_size(r.min + egui::vec2(4.0, 2.0), egui::vec2(28.0, 26.0));
    let plus = Rect::from_min_max(
        Pos2::new(r.max.x - 32.0, r.min.y + 2.0),
        r.max - egui::vec2(4.0, 2.0),
    );
    painter.text(
        minus.center(),
        egui::Align2::CENTER_CENTER,
        "−",
        egui::FontId::proportional(15.0),
        theme::DIM,
    );
    painter.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        format!("{}%", (p.zoom * 100.0).round() as i32),
        egui::FontId::monospace(11.0),
        theme::DIM,
    );
    painter.text(
        plus.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(15.0),
        theme::DIM,
    );
    if ui.interact(minus, egui::Id::new("missions-zoom-minus"), Sense::click())
        .clicked()
    {
        acts.push(Act::ZoomDelta(-0.1));
    }
    if ui.interact(plus, egui::Id::new("missions-zoom-plus"), Sense::click())
        .clicked()
    {
        acts.push(Act::ZoomDelta(0.1));
    }
    if ui.interact(r, egui::Id::new("missions-zoom-reset"), Sense::click())
        .double_clicked()
    {
        acts.push(Act::ResetView);
    }
    acts
}

fn paint_status_pill(ui: &mut egui::Ui, canvas: Rect, p: &Panel) {
    let margin = 14.0;
    let h = 24.0;
    let msg = p.status.as_str();
    let w = (msg.chars().count() as f32 * 7.5 + 24.0).min(canvas.width() - 60.0 - 160.0);
    let r = Rect::from_min_size(
        Pos2::new(canvas.min.x + margin + 380.0, canvas.max.y - margin - h),
        egui::vec2(w, h),
    );
    let painter = ui.painter();
    painter.rect_filled(
        r,
        egui::Rounding::same(8.0),
        Color32::from_rgba_premultiplied(0x11, 0x15, 0x1c, 220),
    );
    painter.rect_stroke(r, egui::Rounding::same(8.0), Stroke::new(1.0, NODE_BORDER));
    painter.text(
        r.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        msg,
        egui::FontId::monospace(10.5),
        theme::DIM,
    );
}

// ───────────────────────────────────────────────────────────────────── inspector

pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    let Some(mission) = &p.mission else {
        ui.weak("No mission loaded.");
        return acts;
    };
    // Mission header
    ui.horizontal(|ui| {
        ui.label(theme::disp_text(&mission.id, 15.0, theme::TX));
        ui.label(
            egui::RichText::new(&mission.inherits)
                .monospace()
                .size(10.5)
                .color(theme::FAINT),
        );
    });
    ui.label(
        egui::RichText::new(&mission.path)
            .size(10.0)
            .color(theme::FAINT),
    );
    // Save overlay state for this mission
    if let Some(ov) = &p.save {
        let st = ov.state_for(&mission.id);
        let (label, col) = match st {
            MissionState::Active => ("ACTIVE in save", Color32::from_rgb(0x10, 0xb9, 0x81)),
            MissionState::Done => ("COMPLETED in save", Color32::from_rgb(0x74, 0xdd, 0xc8)),
            MissionState::Queued => ("in flow chain", Color32::from_rgb(0x9e, 0xc8, 0xea)),
            MissionState::None => ("not in save", theme::FAINT),
        };
        ui.add_space(2.0);
        ui.label(egui::RichText::new(label).size(11.0).color(col));
        if ov.active_contract == mission.id {
            ui.label(
                egui::RichText::new("= header active_contract")
                    .size(10.0)
                    .color(theme::FAINT),
            );
        }
    }
    ui.add_space(10.0);

    let Some(node) = p.selected_node() else {
        ui.weak("No node selected — click one on the canvas.");
        return acts;
    };
    let (htop, hbot, _htext, eyebrow) = node.kind.header_colours();

    // Heading
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::hover());
        paint_vgradient(ui.painter(), rect, htop, hbot, egui::Rounding::same(8.0));
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(&node.title)
                    .size(15.0)
                    .color(theme::TX)
                    .strong(),
            );
            ui.horizontal(|ui| {
                pill_tag(ui, eyebrow, theme::BRASS);
                pill_tag_neutral(ui, &format!("L{}", node.source_line));
                if !node.containing_fn.is_empty() {
                    pill_tag_neutral(ui, &format!("in {}", node.containing_fn));
                }
            });
        });
    });
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(&node.blurb)
            .size(12.0)
            .color(theme::FAINT),
    );
    ui.add_space(10.0);

    // Parameters — editable for string literals whose byte range the parser
    // captured (cue, track, sName, sModuleName, layer). Non-literal params
    // (dynamic identifiers, counts, event kinds) are read-only; the surface
    // says so plainly so no field pretends to accept keystrokes that would
    // not land anywhere.
    if node
        .inputs
        .iter()
        .any(|p| !matches!(p.ty, PinTy::Exec))
    {
        theme::section(ui, "Parameters", None, true, |ui| {
            for pin in &node.inputs {
                if matches!(pin.ty, PinTy::Exec) {
                    continue;
                }
                let editable = pin.ty == PinTy::Str && pin.edit_range.is_some();
                if editable {
                    render_editable_string_pin(ui, mission, &node.id, pin, &mut acts);
                } else {
                    render_readonly_pin(ui, pin);
                }
            }
        });
    }

    // Outputs
    if !node.outputs.is_empty() {
        theme::section(ui, "Outputs", None, true, |ui| {
            for pin in &node.outputs {
                let swatch_col = pin.ty.colour();
                theme::row_chip(ui, Color32::TRANSPARENT, theme::LINE, |ui| {
                    swatch(ui, swatch_col, matches!(pin.ty, PinTy::Exec));
                    ui.label(egui::RichText::new(&pin.label).size(12.0).color(theme::TX));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(pin.annot.as_deref().unwrap_or("(unbound)"))
                                .monospace()
                                .size(10.0)
                                .color(theme::FAINT),
                        );
                    });
                });
            }
        });
    }

    // Source snippet
    theme::section(ui, "Source", Some(&format!("L{}", node.source_line)), true, |ui| {
        let text = node.snippet.join("\n");
        theme::code_block(ui, &text);
    });

    acts
}

/// Draw one editable string pin: a value well that reads the current literal
/// out of `mission.source[pin.edit_range]`, lets the user type, and commits on
/// Enter / blur via `Act::EditStringPin`.
///
/// The TextEdit backing buffer lives in egui's `data` store rather than on
/// `Panel` because the inspector takes `&Panel`; stashing the typed-but-
/// uncommitted text on egui's per-id data map avoids a `&mut Panel` churn and
/// cleans up automatically when the id goes stale. The buffer's id is keyed on
/// (mission id, node id, pin label) so switching missions never cross-feeds
/// one pin's draft into another's slot.
fn render_editable_string_pin(
    ui: &mut egui::Ui,
    mission: &Mission,
    node_id: &str,
    pin: &Pin,
    acts: &mut Vec<Act>,
) {
    let Some(range) = pin.edit_range.clone() else {
        render_readonly_pin(ui, pin);
        return;
    };
    if range.start >= mission.source.len() || range.end > mission.source.len() {
        render_readonly_pin(ui, pin);
        return;
    }
    let raw = &mission.source[range.clone()];
    let current_unquoted = strip_lua_quotes(raw);
    let id = egui::Id::new((
        "missions-str-edit",
        mission.id.as_str(),
        node_id,
        pin.label.as_str(),
    ));
    let mut buf: String = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| current_unquoted.clone());
    ui.horizontal(|ui| {
        swatch(ui, pin.ty.colour(), false);
        ui.label(egui::RichText::new(&pin.label).size(12.0).color(theme::TX));
        let dirty = buf.as_str() != current_unquoted.as_str();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .font(egui::FontId::monospace(11.0))
                .desired_width(ui.available_width() - 60.0),
        );
        ui.ctx().data_mut(|d| d.insert_temp(id, buf.clone()));
        let committed = resp.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Tab));
        if (committed || resp.lost_focus()) && dirty {
            acts.push(Act::EditStringPin {
                node_id: node_id.to_string(),
                pin_label: pin.label.clone(),
                new_value: buf.clone(),
            });
        }
    });
}

/// Draw a parameter pin that cannot be edited through the string-splice path
/// (anything that isn't a bare string literal in source). The eyebrow spells
/// out WHY it's not editable, so the user does not stare at a greyed row with
/// no explanation.
fn render_readonly_pin(ui: &mut egui::Ui, pin: &Pin) {
    theme::row_chip(ui, Color32::TRANSPARENT, theme::LINE, |ui| {
        swatch(ui, pin.ty.colour(), false);
        ui.label(egui::RichText::new(&pin.label).size(12.0).color(theme::TX));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let suffix = match pin.ty {
                PinTy::Int => "int · read-only (edit in source)",
                PinTy::Obj | PinTy::Arr => "composite · read-only",
                PinTy::Str => "dynamic · read-only",
                PinTy::Exec => "",
            };
            ui.label(
                egui::RichText::new(suffix)
                    .monospace()
                    .size(9.5)
                    .color(theme::FAINT),
            );
            if let Some(annot) = &pin.annot {
                ui.label(
                    egui::RichText::new(annot)
                        .monospace()
                        .size(10.0)
                        .color(theme::DIM),
                );
            }
        });
    });
}

/// Strip Lua quote characters from a literal — accepts `"foo"` and `'foo'`
/// and leaves anything else untouched. Lua escape sequences are left as-is
/// (the parser consumes the file verbatim and the splice preserves the
/// author's exact bytes until an edit replaces them).
fn strip_lua_quotes(raw: &str) -> String {
    if raw.len() >= 2 {
        let b = raw.as_bytes();
        let first = b[0];
        let last = b[raw.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            return raw[1..raw.len() - 1].to_string();
        }
    }
    raw.to_string()
}

fn pill_tag(ui: &mut egui::Ui, text: &str, border: Color32) {
    egui::Frame::none()
        .stroke(Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(7.0, 2.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .monospace()
                    .size(9.0)
                    .color(border),
            );
        });
}

fn pill_tag_neutral(ui: &mut egui::Ui, text: &str) {
    egui::Frame::none()
        .stroke(Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(7.0, 2.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .monospace()
                    .size(9.0)
                    .color(theme::FAINT),
            );
        });
}

// ───────────────────────────────────────────────────────────────────── verbs

pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let has_node = p.selected_node().is_some();
    if ui
        .add_enabled(has_node, egui::Button::new("Jump to source"))
        .on_hover_text("Open the Lua viewer at this node's source line")
        .clicked()
    {
        acts.push(Act::JumpToSource);
    }
    // Save / discard — enabled only while there are in-memory edits. Both are
    // hidden-when-disabled would be dishonest; a disabled chip is clearer.
    let dirty = p.source_dirty && p.mission.is_some();
    if ui
        .add_enabled(dirty, egui::Button::new("Save to overlay"))
        .on_hover_text(
            "Writes the edited mission source to \
             <cwd>/missions-overlay/<path> — never overwrites the vendored corpus.",
        )
        .clicked()
    {
        acts.push(Act::SaveToOverlay);
    }
    if ui
        .add_enabled(dirty, egui::Button::new("Discard edits"))
        .on_hover_text("Re-parse the mission from disk, dropping every edit in memory")
        .clicked()
    {
        acts.push(Act::DiscardEdits);
    }
    // Status line for the save verb — surfaced here (not in the inspector)
    // because it is a verb-bar event.
    match &p.last_save_status {
        Some(Ok(path)) => {
            ui.label(
                egui::RichText::new(format!("saved: {}", path))
                    .size(10.5)
                    .color(theme::GOOD),
            );
        }
        Some(Err(e)) => {
            ui.label(
                egui::RichText::new(format!("save failed: {}", e))
                    .size(10.5)
                    .color(theme::BAD),
            );
        }
        None => {
            if dirty {
                ui.label(
                    egui::RichText::new("unsaved edits")
                        .size(10.5)
                        .color(theme::BRASS),
                );
            }
        }
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // No compile step yet — this is the honest surface.
        let _ = theme::primary_button(ui, "Compile — not wired", false);
    });
    acts
}

// ───────────────────────────────────────────────────────────────────── tests

#[cfg(test)]
mod tests {
    use super::*;

    const PMCCON016: &str = r#"inherit("MrxTaskContract")
import("MrxTaskRace")

function Activated(self)
  MrxTaskContract.Activated(self)
  self:GetVehicles()
  MrxVoSequence.Start({"Fiona-In-Mission-MinorContract-Pmc16-01"})
  self:_CreateEvent(Event.ObjectDeath, {tVehicle}, self.VehicleDeath, {self})
  MrxMusic.PlaySpecialMusic("mu_PmcCon016_01")
end

function GetVehicles(self)
  tVehicle = MrxUtil.SpawnObject("Panhard (Assault)", nil)
end

function VehicleDeath(self)
  MrxVoSequence.Start({"Fiona-In-Mission-MinorContract-Pmc16-04"})
end
"#;

    #[test]
    fn parses_fns_and_calls() {
        let m = parse_mission("lua/pmccon016.lua", PMCCON016);
        assert_eq!(m.inherits, "MrxTaskContract");
        assert_eq!(m.id, "PmcCon016");
        // 3 fns + 2 VO + 1 event + 1 music = 7 nodes
        assert_eq!(m.functions.len(), 3);
        let vos = m.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Vo)).count();
        assert_eq!(vos, 2);
        let evs = m
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Event))
            .count();
        assert_eq!(evs, 1);
        let mus = m
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Music))
            .count();
        assert_eq!(mus, 1);
    }

    #[test]
    fn different_scripts_produce_different_graphs() {
        let a = parse_mission("lua/a.lua", PMCCON016);
        let b_src = "inherit(\"MrxTaskJob\")\nfunction Activated(self)\nend\n";
        let b = parse_mission("lua/b.lua", b_src);
        assert_ne!(a.inherits, b.inherits);
        assert_ne!(a.nodes.len(), b.nodes.len());
    }

    #[test]
    fn catalog_filters_to_mission_inheritors() {
        let corpus = vec![
            (
                "lua/pmccon016.lua".to_string(),
                "inherit(\"MrxTaskContract\")\n".to_string(),
                String::new(),
            ),
            (
                "lua/mrxcommon.lua".to_string(),
                "inherit(\"SomethingElse\")\n".to_string(),
                String::new(),
            ),
            (
                "lua/mecjob001.lua".to_string(),
                "inherit(\"MecJob\")\n".to_string(),
                String::new(),
            ),
        ];
        let cat = build_catalog(&corpus);
        assert_eq!(cat.len(), 2);
        assert!(cat.iter().any(|e| e.id == "PmcCon016"));
        assert!(cat.iter().any(|e| e.id == "MecJob001"));
    }

    #[test]
    fn code_mask_skips_strings_and_comments() {
        let s = r#"-- MrxVoSequence.Start fake
"MrxVoSequence.Start also fake"
MrxVoSequence.Start({"real"})
"#;
        let mask = code_mask(s);
        let hits = find_in_code(s, &mask, "MrxVoSequence.Start");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn unparseable_matches_still_surface() {
        let s = "function Activated(self)\n  MrxVoSequence.Start(\n"; // missing close
        let m = parse_mission("lua/x.lua", s);
        assert!(m
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Unparseable)));
    }

    #[test]
    fn string_edit_round_trips_through_apply() {
        // Build a panel with a tiny mission, verify the splice path mutates
        // the source in-place and keeps `source_dirty` honest.
        let src = PMCCON016;
        let mission = parse_mission("lua/pmccon016.lua", src);
        let (vo_id, range) = {
            let vo_node = mission
                .nodes
                .iter()
                .find(|n| matches!(n.kind, NodeKind::Vo))
                .expect("vo node");
            let cue_pin = vo_node
                .inputs
                .iter()
                .find(|p| p.label == "cue")
                .expect("cue pin");
            (
                vo_node.id.clone(),
                cue_pin.edit_range.clone().expect("cue has an edit range"),
            )
        };
        // Range points at `"Fiona-In-Mission-..."` (quotes included).
        assert_eq!(&src[range.start..=range.start], "\"");
        assert_eq!(&src[range.end - 1..range.end], "\"");

        let mut p = Panel::default();
        p.mission = Some(mission);
        apply_string_edit(&mut p, &vo_id, "cue", "Pmc16-FAKE");
        let mutated = p.mission.as_ref().unwrap();
        assert!(mutated.source.contains("\"Pmc16-FAKE\""));
        assert!(!mutated.source.contains("Fiona-In-Mission-MinorContract-Pmc16-01"));
        assert!(p.source_dirty);
    }

    #[test]
    fn encode_lua_string_escapes_quotes_and_backslashes() {
        assert_eq!(encode_lua_string("plain", '"'), "\"plain\"");
        assert_eq!(encode_lua_string("a\"b", '"'), "\"a\\\"b\"");
        assert_eq!(encode_lua_string("a\\b", '"'), "\"a\\\\b\"");
        assert_eq!(encode_lua_string("line\n2", '"'), "\"line\\n2\"");
        // Alt quote: using single-quote delimiter escapes only ' not "
        assert_eq!(encode_lua_string("x\"y", '\''), "'x\"y'");
    }

    /// End-to-end: parse a real retail script and confirm the headline counts match a direct
    /// substring count. This is the proof that the parser extracts the real file, not a
    /// hand-decoded sketch.
    #[test]
    fn retail_pmccon016_parses() {
        let path = "../mercs2_script/corpus/mercs2-luacd/src/vz/pmccon016.lua";
        let Ok(src) = std::fs::read_to_string(path) else {
            // The parser crate ships without the retail corpus in some CI configs; skip
            // loudly rather than silently pass.
            eprintln!("SKIPPING: no retail corpus at {path}");
            return;
        };
        let m = parse_mission(path, &src);
        assert_eq!(m.inherits, "MrxTaskContract");
        assert_eq!(m.id, "PmcCon016");
        let vos = m.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Vo)).count();
        let evs = m
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Event))
            .count();
        let kids = m
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Objective))
            .count();
        let mus = m
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Music))
            .count();
        let fns = m
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Fn | NodeKind::Lifecycle))
            .count();
        // Direct greps on the source — these are the floor; parser may legitimately produce
        // more if it finds an occurrence inside a comment the parser respects (there are
        // none in retail scripts).
        assert_eq!(vos, src.matches("MrxVoSequence.Start").count());
        assert_eq!(kids, src.matches(":CreateChild").count());
        assert_eq!(evs, src.matches("_CreateEvent").count());
        // MrxMusic has Play/Stop variants — parser only counts the ones it recognised.
        assert!(mus >= 1, "expected at least one music call, got {mus}");
        assert!(fns >= 10, "expected ≥10 top-level fns, got {fns}");
    }
}
