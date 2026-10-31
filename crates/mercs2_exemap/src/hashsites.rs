//! Every 32-bit constant of one image's decompiled code and data, with the role it plays where it
//! is used, for recovering the names behind `pandemic_hash_m2` hashes.
//!
//! **Name pool** ([`NamePool`]). Rainbow-table JSON files (`{"pandemic_hash_m2": {"0x%08X":
//! [name, …]}, …}`; other top-level keys are not read). Every name is hashed again on load and a
//! name whose `pandemic_hash_m2` is not its key is an error listing each, as is a key that is not
//! `0x` and eight hex digits, a value that is not an array of strings, and a file without the
//! `pandemic_hash_m2` object. A value in the pool is a known hash *name*, not proof that a constant
//! equal to it is that hash: the pool holds ~7.3·10⁵ hashes, so of 10⁴ unrelated constants about two
//! match by chance.
//!
//! **Decomp sites** ([`scan`]). Each function's decompiled text is tokenized as C (string and char
//! literals, `/* */` and `//` comments, multi-character operators). A hex literal is a constant;
//! a `-` that follows an operator, `(`, `,`, `return` or `case` negates it, and the value is its
//! 32-bit two's complement (Ghidra prints `int` compares as `-0x62568f9b` for `0x9da97065`). A
//! 16-digit literal whose high word is `0xffffffff` is a sign-extended 32-bit value (PowerPC
//! decomp); any other literal over 32 bits, any value below `0x10000`, and any negated literal
//! below `0x10000` (a negative offset, as in `unaff_r2 + -0x7a1c`) is not a site. Each
//! site has a role, decided in this order:
//!
//! | role | when | detail |
//! |---|---|---|
//! | `case` | `case K:` | — |
//! | `cmp` | `x == K` / `x != K` | the left operand, and its definition when it is a local assigned earlier in the function |
//! | `bound` | `x < K`, `<=`, `>`, `>=` | the left operand (a compiled switch's binary search: often a case value + 1) |
//! | `assign` / `store` | `v = K` to a bare local / to any other lvalue | the lvalue |
//! | `return` | `return K` | — |
//! | `arg` | inside a call's parentheses | callee, 0-based argument index, and whether `K` is the whole argument |
//! | `other` | anything else | — |
//!
//! **Strings.** A function's strings are its string literals (the x86 decomp inlines them), every
//! identifier ending `_<8 hex digits>` that is not `FUN_`/`LAB_`/`thunk_` and whose address holds
//! printable text, and, on the console images, every constant whose address holds printable text
//! (Ghidra leaves PowerPC string addresses as numbers) — such a constant is a string, not a site.
//! On PS3, loads through the TOC are evaluated: `*(T *)(unaff_r2 ± K)` reads the word at the
//! function's TOC (its `.opd` descriptor) `± K`, a local assigned such a load holds that word, and
//! `*(T *)(local ± K)` reads through it; a word that addresses printable text is a string.
//! Printable text is 2..=256 bytes of 0x20..=0x7e followed by a NUL.
//!
//! **Data words.** Every 4-aligned word of the image's non-executable sections (in its byte order)
//! whose value is in the pool is a data hit. A **table** is a run of slots at one stride (4..=64,
//! step 4) through data hits: the run extends over slots whose word is a pool hash or is
//! hash-shaped (at least `0x10000`, not [`structured`], not an address the image maps), is trimmed
//! to its first and last pool hash, holds at least three pool hashes, and pool hashes are at least
//! half its slots. Each hit belongs to the
//! first stride, ascending, whose run reaches it. A table's unknown slots are hash candidates, and
//! the functions whose text addresses any byte of the table (identifier suffixes, mapped
//! constants, PS3 TOC loads) are its `referenced_by`.
//!
//! **Output** (JSON Lines, [`Scan::to_jsonl`]): an image record, one record per distinct constant
//! ascending (`"kind":"value"`, its pool names and every site), one per table, one per function
//! that has a site or references a table (its strings, callers and callees), and an end record
//! with the counts (`*_count` fields).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use mercs2_formats::hash::pandemic_hash_m2_bytes;

use crate::mem::Mem;
use crate::Error;

/// Hashes with every name that hashes to them, from rainbow-table files.
#[derive(Default)]
pub struct NamePool {
    by_hash: HashMap<u32, Vec<String>>,
}

impl NamePool {
    /// Add the `pandemic_hash_m2` object of one rainbow-table file (`what` names it in errors).
    /// Returns how many names were read.
    pub fn add_file(&mut self, text: &str, what: &str) -> Result<usize, Error> {
        let root: Value = serde_json::from_str(text).map_err(|e| Error::new(format!("{what}: {e}")))?;
        let obj = root
            .get("pandemic_hash_m2")
            .and_then(Value::as_object)
            .ok_or_else(|| Error::new(format!("{what}: no \"pandemic_hash_m2\" object")))?;
        let mut bad = Vec::new();
        let mut n = 0usize;
        for (key, names) in obj {
            let h = parse_hash_key(key).ok_or_else(|| Error::new(format!("{what}: key {key:?} is not 0x and eight hex digits")))?;
            let names = names
                .as_array()
                .ok_or_else(|| Error::new(format!("{what}: {key} is not an array of names")))?;
            for name in names {
                let s = name.as_str().ok_or_else(|| Error::new(format!("{what}: {key} holds a name that is not a string")))?;
                if pandemic_hash_m2_bytes(s.as_bytes()) != h {
                    bad.push(format!("{key} {s:?}"));
                    continue;
                }
                n += 1;
                let e = self.by_hash.entry(h).or_default();
                if !e.iter().any(|x| x == s) {
                    e.push(s.to_string());
                }
            }
        }
        if !bad.is_empty() {
            return Err(Error::new(format!(
                "{what}: {} names do not hash to their key: {}",
                bad.len(),
                bad.join(", ")
            )));
        }
        Ok(n)
    }

    pub fn get(&self, h: u32) -> Option<&[String]> {
        self.by_hash.get(&h).map(Vec::as_slice)
    }

    pub fn contains(&self, h: u32) -> bool {
        self.by_hash.contains_key(&h)
    }

    /// Distinct hashes in the pool.
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }
}

fn parse_hash_key(key: &str) -> Option<u32> {
    let h = key.strip_prefix("0x").or_else(|| key.strip_prefix("0X"))?;
    if h.len() != 8 {
        return None;
    }
    u32::from_str_radix(h, 16).ok()
}

// ---------------------------------------------------------------------------------------------
// Tokenizer

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tok {
    Ident(String),
    /// A numeric literal: its value (when it fits 64 bits) and whether it was written in hex.
    Num(Option<u64>, bool),
    Str(Vec<u8>),
    Char,
    Punct(&'static str),
}

const PUNCTS: [&str; 46] = [
    "<<=", ">>=", "...", "==", "!=", "<=", ">=", "&&", "||", "->", "<<", ">>", "++", "--", "+=", "-=", "*=", "/=", "%=",
    "&=", "|=", "^=", "::", "(", ")", "{", "}", "[", "]", ";", ",", ".", "?", ":", "=", "<", ">", "+", "-", "*", "/",
    "%", "&", "|", "^", "!",
];

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Tokenize decompiled C. Bytes that start no token (a lone `~`, `#`, `@`, `$`, `\`) are skipped;
/// the role grammar never depends on them.
pub(crate) fn tokenize(src: &str) -> Vec<Tok> {
    let s = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let b = s[i];
        if b.is_ascii_whitespace() {
            i += 1;
        } else if s[i..].starts_with(b"/*") {
            i = match src[i + 2..].find("*/") {
                Some(k) => i + 2 + k + 2,
                None => s.len(),
            };
        } else if s[i..].starts_with(b"//") {
            while i < s.len() && s[i] != b'\n' {
                i += 1;
            }
        } else if b == b'"' {
            let (bytes, end) = string_literal(s, i + 1, b'"');
            out.push(Tok::Str(bytes));
            i = end;
        } else if b == b'\'' {
            let (_, end) = string_literal(s, i + 1, b'\'');
            out.push(Tok::Char);
            i = end;
        } else if b.is_ascii_digit() {
            let start = i;
            let hex = s[i..].starts_with(b"0x") || s[i..].starts_with(b"0X");
            if hex {
                i += 2;
                while i < s.len() && s[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let v = u64::from_str_radix(&src[start + 2..i], 16).ok();
                while i < s.len() && matches!(s[i], b'u' | b'U' | b'l' | b'L') {
                    i += 1;
                }
                out.push(Tok::Num(v, true));
            } else {
                while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'.') {
                    i += 1;
                }
                let text = &src[start..i];
                let v = text.trim_end_matches(['u', 'U', 'l', 'L']).parse::<u64>().ok();
                out.push(Tok::Num(v, false));
            }
        } else if is_ident_start(b) {
            let start = i;
            while i < s.len() && is_ident(s[i]) {
                i += 1;
            }
            out.push(Tok::Ident(src[start..i].to_string()));
        } else if let Some(p) = PUNCTS.iter().find(|p| s[i..].starts_with(p.as_bytes())) {
            out.push(Tok::Punct(p));
            i += p.len();
        } else {
            i += 1;
        }
    }
    out
}

/// The bytes of a literal whose opening quote precedes `i`, with C escapes decoded, and the index
/// after its closing quote.
fn string_literal(s: &[u8], mut i: usize, quote: u8) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    while i < s.len() && s[i] != quote {
        if s[i] == b'\\' && i + 1 < s.len() {
            i += 1;
            match s[i] {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'0' => out.push(0),
                b'x' => {
                    let mut v = 0u32;
                    let mut k = 0;
                    while k < 2 && i + 1 < s.len() && s[i + 1].is_ascii_hexdigit() {
                        i += 1;
                        v = v * 16 + (s[i] as char).to_digit(16).unwrap_or(0);
                        k += 1;
                    }
                    out.push(v as u8);
                }
                c => out.push(c),
            }
        } else {
            out.push(s[i]);
        }
        i += 1;
    }
    (out, (i + 1).min(s.len()))
}

const KEYWORDS: [&str; 7] = ["if", "while", "for", "switch", "return", "sizeof", "do"];

fn punct(t: Option<&Tok>, p: &str) -> bool {
    matches!(t, Some(Tok::Punct(q)) if *q == p)
}

/// Does a `-` at token `i` negate the operand after it (rather than subtract)?
fn unary_minus_at(toks: &[Tok], i: usize) -> bool {
    if !punct(toks.get(i), "-") {
        return false;
    }
    match i.checked_sub(1).and_then(|j| toks.get(j)) {
        None => true,
        Some(Tok::Ident(w)) => w == "return" || w == "case",
        Some(Tok::Num(..)) | Some(Tok::Str(_)) | Some(Tok::Char) => false,
        Some(Tok::Punct(p)) => !matches!(*p, ")" | "]"),
    }
}

/// The 32-bit value of a hex literal: two's complement when negated, the low word of a
/// sign-extended 64-bit literal; `None` for anything wider.
pub(crate) fn literal_u32(v: u64, negated: bool) -> Option<u32> {
    if negated {
        (v <= 0x8000_0000).then(|| (v as u32).wrapping_neg())
    } else if v <= 0xffff_ffff || v >> 32 == 0xffff_ffff {
        Some(v as u32)
    } else {
        None
    }
}

/// Index of the unmatched `(` enclosing token `end` within its statement.
fn enclosing_open(toks: &[Tok], end: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut j = end;
    while j > 0 {
        j -= 1;
        match &toks[j] {
            Tok::Punct(")") => depth += 1,
            Tok::Punct("(") => {
                if depth == 0 {
                    return Some(j);
                }
                depth -= 1;
            }
            Tok::Punct(";") | Tok::Punct("{") | Tok::Punct("}") if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

/// Start index of the expression that ends just before token `end` (exclusive), at depth 0:
/// back to an unmatched `(` or `[`, or a `,` `;` `{` `}` `&&` `||` `?` `:` `=` `return` `case`.
fn expr_start(toks: &[Tok], end: usize) -> usize {
    let mut depth = 0i32;
    let mut j = end;
    while j > 0 {
        let t = &toks[j - 1];
        match t {
            Tok::Punct(")") | Tok::Punct("]") => depth += 1,
            Tok::Punct("(") | Tok::Punct("[") => {
                if depth == 0 {
                    return j;
                }
                depth -= 1;
            }
            Tok::Punct(p) if depth == 0 && matches!(*p, "," | ";" | "{" | "}" | "&&" | "||" | "?" | ":" | "=" | "==" | "!=") => {
                return j
            }
            Tok::Ident(w) if depth == 0 && (w == "return" || w == "case") => return j,
            _ => {}
        }
        j -= 1;
    }
    0
}

/// Render tokens as one deterministic line of text.
pub(crate) fn render(toks: &[Tok]) -> String {
    let mut out = String::new();
    for (k, t) in toks.iter().enumerate() {
        let text = match t {
            Tok::Ident(w) => w.clone(),
            Tok::Num(Some(v), true) => format!("0x{v:x}"),
            Tok::Num(Some(v), false) => v.to_string(),
            Tok::Num(None, _) => "?".into(),
            Tok::Str(b) => format!("{:?}", String::from_utf8_lossy(b)),
            Tok::Char => "'?'".into(),
            Tok::Punct(p) => (*p).to_string(),
        };
        let tight_before = matches!(t, Tok::Punct(")") | Tok::Punct(",") | Tok::Punct("]") | Tok::Punct(";"));
        let tight_after_prev = k > 0 && matches!(&toks[k - 1], Tok::Punct("(") | Tok::Punct("["))
            || (k > 0 && matches!(t, Tok::Punct("(")) && matches!(&toks[k - 1], Tok::Ident(_) | Tok::Punct(")")));
        if k > 0 && !tight_before && !tight_after_prev {
            out.push(' ');
        }
        out.push_str(&text);
    }
    out
}

/// The expression assigned to local `name` by the last statement `name = …;` before token `before`
/// that is not inside a block closed before `before` (a sibling branch's assignment is not the
/// value the site sees).
fn local_definition(toks: &[Tok], name: &str, before: usize) -> Option<String> {
    let mut j = before;
    let mut closed = 0usize;
    while j > 1 {
        j -= 1;
        match &toks[j] {
            Tok::Punct("}") => closed += 1,
            Tok::Punct("{") => closed = closed.saturating_sub(1),
            _ => {}
        }
        if closed == 0
            && matches!(&toks[j - 1], Tok::Ident(w) if w == name)
            && punct(toks.get(j), "=")
            && (j < 2 || matches!(&toks[j - 2], Tok::Punct(";") | Tok::Punct("{") | Tok::Punct("}")))
        {
            let end = toks[j + 1..before].iter().position(|t| punct(Some(t), ";"))? + j + 1;
            return Some(render(&toks[j + 1..end]));
        }
    }
    None
}

fn is_local_name(toks: &[Tok]) -> Option<&str> {
    match toks {
        [Tok::Ident(w)] if !KEYWORDS.contains(&w.as_str()) => Some(w),
        _ => None,
    }
}

/// Where a constant is used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Role {
    Case,
    Cmp {
        op: String,
        lhs: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        lhs_def: Option<String>,
    },
    Bound {
        op: String,
        lhs: String,
    },
    Assign {
        lhs: String,
    },
    Store {
        lhs: String,
    },
    Return,
    Arg {
        callee: String,
        index: usize,
        whole: bool,
    },
    Other,
}

/// The role of the constant whose literal spans tokens `start..=end` (`start` is the unary minus
/// when there is one).
pub(crate) fn role_of(toks: &[Tok], start: usize, end: usize) -> Role {
    let prev = start.checked_sub(1).map(|j| &toks[j]);
    match prev {
        Some(Tok::Ident(w)) if w == "case" => return Role::Case,
        Some(Tok::Ident(w)) if w == "return" => return Role::Return,
        Some(Tok::Punct(op)) if matches!(*op, "==" | "!=") => {
            let lhs_end = start - 1;
            let ls = expr_start(toks, lhs_end);
            let lhs_toks = &toks[ls..lhs_end];
            let lhs_def = is_local_name(lhs_toks).and_then(|n| local_definition(toks, n, ls));
            return Role::Cmp { op: (*op).to_string(), lhs: render(lhs_toks), lhs_def };
        }
        Some(Tok::Punct(op)) if matches!(*op, "<" | "<=" | ">" | ">=") => {
            let ls = expr_start(toks, start - 1);
            return Role::Bound { op: (*op).to_string(), lhs: render(&toks[ls..start - 1]) };
        }
        Some(Tok::Punct("=")) => {
            let ls = expr_start(toks, start - 1);
            let lhs_toks = &toks[ls..start - 1];
            let lhs = render(lhs_toks);
            return if is_local_name(lhs_toks).is_some() { Role::Assign { lhs } } else { Role::Store { lhs } };
        }
        _ => {}
    }
    // the innermost enclosing parentheses that a callee name opens; grouping parentheses are
    // walked out of, a keyword's (`if (`, `while (`) end the search
    let mut open = enclosing_open(toks, start);
    while let Some(o) = open {
        match o.checked_sub(1).map(|j| &toks[j]) {
            Some(Tok::Ident(_)) => break,
            _ => open = enclosing_open(toks, o),
        }
    }
    if let Some(open) = open {
        if let Some(Tok::Ident(callee)) = open.checked_sub(1).map(|j| &toks[j]) {
            if !KEYWORDS.contains(&callee.as_str()) {
                let mut depth = 0i32;
                let mut index = 0usize;
                for t in &toks[open + 1..start] {
                    match t {
                        Tok::Punct("(") | Tok::Punct("[") => depth += 1,
                        Tok::Punct(")") | Tok::Punct("]") => depth -= 1,
                        Tok::Punct(",") if depth == 0 => index += 1,
                        _ => {}
                    }
                }
                let whole = matches!(&toks[start - 1], Tok::Punct("(") | Tok::Punct(","))
                    && matches!(toks.get(end + 1), Some(Tok::Punct(",")) | Some(Tok::Punct(")")));
                return Role::Arg { callee: callee.clone(), index, whole };
            }
        }
    }
    Role::Other
}

// ---------------------------------------------------------------------------------------------
// Image memory

/// What the scan reads from an image: mapped words, text, and (PS3) each function's TOC.
pub trait Memory {
    /// The word at `va` in the image's byte order, when mapped.
    fn word(&self, va: u32) -> Option<u32>;
    /// Printable text at `va` (see the module documentation).
    fn text(&self, va: u32) -> Option<String>;
    /// Does the image map `va`?
    fn mapped(&self, va: u32) -> bool;
    /// The TOC value of the function whose code starts at `va` (PS3 `.opd`); `None` elsewhere.
    fn toc(&self, va: u32) -> Option<u32>;
    /// The word at `va` when a non-executable section maps it.
    fn data_word(&self, va: u32) -> Option<u32>;
    /// Every 4-aligned word of the non-executable sections, as (va, value), ascending.
    fn data_words(&self) -> Box<dyn Iterator<Item = (u32, u32)> + '_>;
}

const TEXT_MIN: usize = 2;
const TEXT_MAX: usize = 256;

fn printable(b: &[u8]) -> Option<String> {
    (b.len() >= TEXT_MIN && b.len() <= TEXT_MAX && b.iter().all(|&c| (0x20..=0x7e).contains(&c)))
        .then(|| String::from_utf8_lossy(b).into_owned())
}

/// [`Memory`] over a PE or PS3 ELF image ([`Mem`]).
pub struct ImageMemory<'a> {
    mem: Mem<'a>,
    tocs: HashMap<u32, u32>,
}

impl<'a> ImageMemory<'a> {
    pub fn of(image: &'a [u8]) -> Result<ImageMemory<'a>, Error> {
        let mem = Mem::of(image)?;
        let tocs = mem.opd_tocs();
        Ok(ImageMemory { mem, tocs })
    }
}

impl Memory for ImageMemory<'_> {
    fn word(&self, va: u32) -> Option<u32> {
        self.mem.word(va)
    }
    fn text(&self, va: u32) -> Option<String> {
        printable(self.mem.cstr(va, TEXT_MAX)?)
    }
    fn mapped(&self, va: u32) -> bool {
        self.mem.off(va).is_some()
    }
    fn toc(&self, va: u32) -> Option<u32> {
        self.tocs.get(&va).copied()
    }
    fn data_word(&self, va: u32) -> Option<u32> {
        if !va.is_multiple_of(4) || self.mem.exec(va) {
            return None;
        }
        self.mem.word(va)
    }
    fn data_words(&self) -> Box<dyn Iterator<Item = (u32, u32)> + '_> {
        Box::new(self.mem.data_words())
    }
}

// ---------------------------------------------------------------------------------------------
// One function

/// One constant use in a function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    pub function: String,
    #[serde(flatten)]
    pub role: Role,
}

/// What one function's text yields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionScan {
    /// (value, role), in text order.
    pub sites: Vec<(u32, Role)>,
    pub strings: BTreeSet<String>,
    /// Data addresses the text names (identifier suffixes, mapped constants, TOC loads).
    pub addresses: BTreeSet<u32>,
}

/// The address an identifier like `DAT_00b3c4d0` / `s_Enter_00b3c4d0` names, unless it names code.
fn ident_address(w: &str) -> Option<u32> {
    if w.starts_with("FUN_") || w.starts_with("LAB_") || w.starts_with("thunk_") || w.starts_with("switchD_") || w.starts_with("caseD_") {
        return None;
    }
    let (_, tail) = w.rsplit_once('_')?;
    if tail.len() != 8 || !tail.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(tail, 16).ok()
}

/// Smallest value that is a site.
pub const MIN_SITE: u32 = 0x10000;

/// Scan one function's decompiled text. `pointer_strings`: constants that address printable text
/// are strings, not sites (the console images, whose decomp leaves string addresses as numbers).
pub fn scan_function(decomp: &str, va: u32, mem: &dyn Memory, pointer_strings: bool) -> FunctionScan {
    let toks = tokenize(decomp);
    let mut out = FunctionScan::default();
    let toc = mem.toc(va);
    // PS3: locals holding a word loaded through the TOC.
    let mut held: HashMap<String, u32> = HashMap::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Str(b) => {
                if let Some(s) = printable(b) {
                    out.strings.insert(s);
                }
            }
            Tok::Ident(w) => {
                if let Some(a) = ident_address(w) {
                    if mem.mapped(a) {
                        out.addresses.insert(a);
                        if let Some(s) = mem.text(a) {
                            out.strings.insert(s);
                        }
                    }
                }
                // `( base ± K )` closing a `*(T *)` dereference.
                if let Some(word) = toc_load(&toks, i, toc, &held, mem) {
                    out.addresses.insert(word);
                    if let Some(s) = mem.text(word) {
                        out.strings.insert(s);
                    }
                    // `local = (cast)? *(T *)(base ± K);`
                    if let Some(local) = assigned_local(&toks, i) {
                        held.insert(local, word);
                    }
                }
            }
            Tok::Num(Some(v), true) => {
                let negated = i > 0 && unary_minus_at(&toks, i - 1);
                let start = if negated { i - 1 } else { i };
                if let Some(value) = literal_u32(*v, negated).filter(|_| !negated || *v >= MIN_SITE as u64) {
                    let is_text = pointer_strings && !negated && mem.text(value).is_some();
                    if is_text {
                        out.strings.insert(mem.text(value).unwrap_or_default());
                        out.addresses.insert(value);
                    } else {
                        if mem.mapped(value) && !negated {
                            out.addresses.insert(value);
                        }
                        if value >= MIN_SITE {
                            out.sites.push((value, role_of(&toks, start, i)));
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// At an identifier `base` at `i` in `( base ± K )` that follows `*(T *)`, the word loaded: `base`
/// is `unaff_r2` (the TOC) or a local holding a TOC-loaded word.
fn toc_load(toks: &[Tok], i: usize, toc: Option<u32>, held: &HashMap<String, u32>, mem: &dyn Memory) -> Option<u32> {
    let Tok::Ident(base) = &toks[i] else { return None };
    let base_val = if base == "unaff_r2" { toc? } else { *held.get(base)? };
    if !(punct(toks.get(i.checked_sub(1)?), "(") && punct(toks.get(i.checked_sub(2)?), ")") && punct(toks.get(i.checked_sub(3)?), "*")) {
        return None;
    }
    let (sign, k_at) = match (toks.get(i + 1), toks.get(i + 2)) {
        (Some(Tok::Punct("+")), Some(Tok::Punct("-"))) => (-1i64, i + 3),
        (Some(Tok::Punct("+")), _) => (1, i + 2),
        (Some(Tok::Punct("-")), _) => (-1, i + 2),
        _ => return None,
    };
    let Some(Tok::Num(Some(k), _)) = toks.get(k_at) else { return None };
    if !punct(toks.get(k_at + 1), ")") {
        return None;
    }
    let addr = (base_val as i64 + sign * *k as i64) as u32;
    mem.word(addr)
}

/// The local a statement starting `local = ` assigns, when the load at `i` is that statement's
/// whole right-hand side (after casts).
fn assigned_local(toks: &[Tok], i: usize) -> Option<String> {
    let mut j = i;
    let mut eq = None;
    while j > 0 {
        j -= 1;
        match &toks[j] {
            Tok::Punct("=") => {
                eq = Some(j);
                break;
            }
            Tok::Punct(";") | Tok::Punct("{") | Tok::Punct("}") => return None,
            _ => {}
        }
    }
    let eq = eq?;
    let Some(Tok::Ident(local)) = eq.checked_sub(1).map(|k| &toks[k]) else { return None };
    if eq >= 2 && !matches!(&toks[eq - 2], Tok::Punct(";") | Tok::Punct("{") | Tok::Punct("}")) {
        return None;
    }
    // the load's closing `)` must end the statement
    let close = (i + 1..toks.len()).find(|&k| punct(toks.get(k), ")"))?;
    punct(toks.get(close + 1), ";").then(|| local.clone())
}

// ---------------------------------------------------------------------------------------------
// Data tables

/// A run of hash slots at one stride in a data section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub va: u32,
    pub stride: u32,
    /// (slot va, value)
    pub slots: Vec<(u32, u32)>,
}

/// Is `v` structured — a value whose bit pattern says it is not a hash: 12 or more trailing zero
/// bits, a complement below `0x10000`, a float of magnitude `[1e-6, 1e7]` that has a 12-bit-zero
/// mantissa tail or is exactly the `f32` nearest a decimal of at most five significant digits and
/// four decimal places (`0.1`, `2.75`, `1500`), or four printable ASCII bytes (a FourCC)? A random
/// hash is structured with probability about 2%.
pub fn structured(v: u32) -> bool {
    if v.trailing_zeros() >= 12 || !v < 0x10000 {
        return true;
    }
    let f = f32::from_bits(v);
    if f.is_finite() && (1e-6..=1e7).contains(&f.abs()) {
        if v & 0xfff == 0 {
            return true;
        }
        for k in 0..=4 {
            let scale = 10f64.powi(k);
            let digits = (f as f64 * scale).round();
            if digits.abs() < 100_000.0 && ((digits / scale) as f32).to_bits() == v {
                return true;
            }
        }
    }
    v.to_le_bytes().iter().all(|b| (0x20..=0x7e).contains(b))
}

fn hash_shaped(v: u32, mem: &dyn Memory) -> bool {
    v >= MIN_SITE && !structured(v) && !mem.mapped(v)
}

/// Every table of the image's data words (see the module documentation), from the data hits
/// `hits` (the slot addresses whose word is a pool hash, ascending).
pub fn tables(hits: &[u32], pool: &NamePool, mem: &dyn Memory) -> Vec<Table> {
    let ok = |va: u32| mem.data_word(va).is_some_and(|v| pool.contains(v) || hash_shaped(v, mem));
    let mut claimed: BTreeSet<u32> = BTreeSet::new();
    let mut out = Vec::new();
    for &h in hits {
        if claimed.contains(&h) {
            continue;
        }
        for stride in (4..=64).step_by(4) {
            let mut lo = h;
            while lo >= stride && ok(lo - stride) {
                lo -= stride;
            }
            let mut hi = h;
            while hi.checked_add(stride).is_some_and(ok) {
                hi += stride;
            }
            let mut slots: Vec<(u32, u32)> =
                (0..=(hi - lo) / stride).map(|k| lo + k * stride).filter_map(|va| mem.data_word(va).map(|v| (va, v))).collect();
            // a run ends on pool hashes: hash-shaped words past its outermost ones are not its slots
            let first = slots.iter().position(|(_, v)| pool.contains(*v)).unwrap_or(0);
            let last = slots.iter().rposition(|(_, v)| pool.contains(*v)).unwrap_or(0);
            slots = slots[first..=last].to_vec();
            let lo = slots[0].0;
            let known = slots.iter().filter(|(_, v)| pool.contains(*v)).count();
            if known >= 3 && known * 2 >= slots.len() {
                for (va, _) in &slots {
                    claimed.insert(*va);
                }
                out.push(Table { va: lo, stride, slots });
                break;
            }
        }
    }
    out.sort_by_key(|t| (t.va, t.stride));
    out
}

// ---------------------------------------------------------------------------------------------
// The image

/// One function record of a functions export, with the fields the scan reads.
#[derive(Deserialize)]
struct FunctionRecord {
    kind: String,
    va: Option<String>,
    name: Option<String>,
    decomp: Option<String>,
    callers: Option<Vec<Caller>>,
    callees: Option<Vec<String>>,
}

/// One caller of a function: the call site and the function holding it (`null` for a site that
/// Ghidra puts in no function).
#[derive(Deserialize)]
struct Caller {
    site: String,
    function: Option<String>,
}

impl Caller {
    /// The calling function, or `site:<address>` for a call site in no function.
    fn label(self) -> String {
        self.function.unwrap_or_else(|| format!("site:{}", self.site))
    }
}

/// The context of one function with a site or a table reference.
pub struct Context {
    pub name: String,
    pub strings: BTreeSet<String>,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
}

/// One image's scan.
pub struct Scan {
    pub image_id: String,
    pub sha256: String,
    pub functions: usize,
    pub without_decomp: Vec<u32>,
    /// value → sites
    pub values: BTreeMap<u32, Vec<Site>>,
    pub tables: Vec<(Table, Vec<u32>)>,
    /// Every function with a site or a table reference, by va.
    pub context: BTreeMap<u32, Context>,
    pub data_hits: usize,
}

/// Scan every function of a functions export (already checked complete by
/// [`crate::annotate::read_functions`]) and the image's data sections.
pub fn scan(image_id: &str, sha256: &str, functions_text: &str, mem: &dyn Memory, pointer_strings: bool, pool: &NamePool) -> Result<Scan, Error> {
    let mut values: BTreeMap<u32, Vec<Site>> = BTreeMap::new();
    let mut per_fn: BTreeMap<u32, (String, FunctionScan, Vec<String>, Vec<String>)> = BTreeMap::new();
    let mut without_decomp = Vec::new();
    let mut functions = 0usize;
    for (n, line) in functions_text.lines().enumerate() {
        let r: FunctionRecord = serde_json::from_str(line).map_err(|e| Error::new(format!("functions line {}: {e}", n + 1)))?;
        if r.kind != "function" {
            continue;
        }
        functions += 1;
        let va_text = r.va.ok_or_else(|| Error::new(format!("functions line {}: no va", n + 1)))?;
        let va = crate::xmap::parse_address(&va_text)?;
        let Some(decomp) = r.decomp else {
            without_decomp.push(va);
            continue;
        };
        let f = scan_function(&decomp, va, mem, pointer_strings);
        for (value, role) in &f.sites {
            values.entry(*value).or_default().push(Site { function: va_text.clone(), role: role.clone() });
        }
        let callers = r.callers.unwrap_or_default().into_iter().map(Caller::label).collect();
        per_fn.insert(va, (r.name.unwrap_or_default(), f, callers, r.callees.unwrap_or_default()));
    }

    let mut hits: Vec<u32> = mem.data_words().filter(|(_, v)| pool.contains(*v)).map(|(va, _)| va).collect();
    hits.sort_unstable();
    let data_hits = hits.len();
    let found = tables(&hits, pool, mem);
    let mut tables_out = Vec::new();
    for t in found {
        let end = t.slots.last().map(|(va, _)| va + 4).unwrap_or(t.va);
        let refs: Vec<u32> = per_fn
            .iter()
            .filter(|(_, (_, f, _, _))| f.addresses.range(t.va..end).next().is_some())
            .map(|(va, _)| *va)
            .collect();
        tables_out.push((t, refs));
    }

    let referencing: BTreeSet<u32> = tables_out.iter().flat_map(|(_, r)| r.iter().copied()).collect();
    let mut context = BTreeMap::new();
    for (va, (name, f, callers, callees)) in per_fn {
        if !f.sites.is_empty() || referencing.contains(&va) {
            context.insert(va, Context { name, strings: f.strings, callers: dedup(callers), callees: dedup(callees) });
        }
    }
    Ok(Scan {
        image_id: image_id.to_string(),
        sha256: sha256.to_string(),
        functions,
        without_decomp,
        values,
        tables: tables_out,
        context,
        data_hits,
    })
}

fn dedup(v: Vec<String>) -> Vec<String> {
    let s: BTreeSet<String> = v.into_iter().collect();
    s.into_iter().collect()
}

fn hex(v: u32) -> String {
    format!("0x{v:08x}")
}

impl Scan {
    /// The JSON Lines output (see the module documentation).
    pub fn to_jsonl(&self, pool: &NamePool) -> String {
        let mut out = String::new();
        let mut line = |v: Value| {
            out.push_str(&v.to_string());
            out.push('\n');
        };
        line(json!({
            "kind": "image",
            "image_id": self.image_id,
            "sha256": self.sha256,
            "functions": self.functions,
            "functions_without_decomp": self.without_decomp.iter().map(|v| hex(*v)).collect::<Vec<_>>(),
            "pool_hashes": pool.len(),
        }));
        for (value, sites) in &self.values {
            line(json!({
                "kind": "value",
                "value": hex(*value),
                "names": pool.get(*value).unwrap_or(&[]),
                "sites": sites,
            }));
        }
        for (t, refs) in &self.tables {
            line(json!({
                "kind": "table",
                "va": hex(t.va),
                "stride": t.stride,
                "slots": t.slots.iter().map(|(va, v)| json!({"va": hex(*va), "value": hex(*v), "names": pool.get(*v).unwrap_or(&[])})).collect::<Vec<_>>(),
                "referenced_by": refs.iter().map(|v| hex(*v)).collect::<Vec<_>>(),
            }));
        }
        for (va, c) in &self.context {
            line(json!({
                "kind": "function",
                "va": hex(*va),
                "name": c.name,
                "strings": c.strings,
                "callers": c.callers,
                "callees": c.callees,
            }));
        }
        let known = self.values.keys().filter(|v| pool.contains(**v)).count();
        let table_unknown: usize = self.tables.iter().map(|(t, _)| t.slots.iter().filter(|(_, v)| !pool.contains(*v)).count()).sum();
        line(json!({
            "kind": "end",
            "value_count": self.values.len(),
            "values_in_pool_count": known,
            "site_count": self.values.values().map(Vec::len).sum::<usize>(),
            "data_hit_count": self.data_hits,
            "table_count": self.tables.len(),
            "table_unknown_slot_count": table_unknown,
            "function_count": self.context.len(),
        }));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeMem {
        words: HashMap<u32, u32>,
        text: HashMap<u32, &'static str>,
        lo: u32,
        hi: u32,
        tocs: HashMap<u32, u32>,
    }

    impl Memory for FakeMem {
        fn word(&self, va: u32) -> Option<u32> {
            self.words.get(&va).copied()
        }
        fn text(&self, va: u32) -> Option<String> {
            self.text.get(&va).map(|s| s.to_string())
        }
        fn mapped(&self, va: u32) -> bool {
            va >= self.lo && va < self.hi
        }
        fn toc(&self, va: u32) -> Option<u32> {
            self.tocs.get(&va).copied()
        }
        fn data_word(&self, va: u32) -> Option<u32> {
            self.words.get(&va).copied()
        }
        fn data_words(&self) -> Box<dyn Iterator<Item = (u32, u32)> + '_> {
            let mut v: Vec<(u32, u32)> = self.words.iter().map(|(a, b)| (*a, *b)).collect();
            v.sort_unstable();
            Box::new(v.into_iter())
        }
    }

    fn empty() -> FakeMem {
        FakeMem { words: HashMap::new(), text: HashMap::new(), lo: 0x0040_0000, hi: 0x0050_0000, tocs: HashMap::new() }
    }

    // The game-state switch of pc-v11 FUN_005f06c0, abridged.
    const STATE: &str = r#"
void __fastcall FUN_005f06c0(int param_1)
{
  uint uVar1;
  int iVar2;
  char *pcVar3;
  uVar1 = *(uint *)(param_1 + 0x24);
  if (uVar1 < 0xc5498140) {
    if (uVar1 == 0xc549813f) {
      pcVar3 = "SupportMenu";
    }
  }
  FUN_0085dc10(pcVar3);
  iVar2 = *(int *)(param_1 + 0x28);
  if (iVar2 == -0x62568f9b) {
    iVar2 = FUN_0085d7f0();
    FUN_0085dc10("Enter");
  }
  else if (iVar2 == -0x24befe83) {
    FUN_0085dc10("Exit");
  }
  switch(iVar2) { case 0x12345678: break; }
  FUN_00401000(param_1, 0x9da97065, (0x51bff7b1 + 1));
  *(int *)(param_1 + 0x30) = 0xdb41017d;
  return 0x7d0b162c;
}
"#;

    #[test]
    fn negated_and_sign_extended_literals_are_their_32_bit_values() {
        assert_eq!(literal_u32(0x62568f9b, true), Some(0x9da97065));
        assert_eq!(literal_u32(0xffff_ffff_820d_9408, false), Some(0x820d9408));
        assert_eq!(literal_u32(0x1_0000_0000, false), None);
        assert_eq!(literal_u32(0x8000_0001, true), None);
    }

    #[test]
    fn every_role_is_recognised() {
        let f = scan_function(STATE, 0x005f06c0, &empty(), false);
        let get = |v: u32| f.sites.iter().filter(|(x, _)| *x == v).map(|(_, r)| r.clone()).collect::<Vec<_>>();
        assert_eq!(get(0xc5498140), vec![Role::Bound { op: "<".into(), lhs: "uVar1".into() }]);
        assert_eq!(
            get(0xc549813f),
            vec![Role::Cmp { op: "==".into(), lhs: "uVar1".into(), lhs_def: Some("* (uint *)(param_1 + 0x24)".into()) }]
        );
        assert_eq!(
            get(0x9da97065)[0],
            Role::Cmp { op: "==".into(), lhs: "iVar2".into(), lhs_def: Some("* (int *)(param_1 + 0x28)".into()) }
        );
        // the assignment inside the `if` body is a sibling branch's, not the else-if's operand
        assert_eq!(
            get(0xdb41017d)[0],
            Role::Cmp { op: "==".into(), lhs: "iVar2".into(), lhs_def: Some("* (int *)(param_1 + 0x28)".into()) }
        );
        assert_eq!(get(0x12345678), vec![Role::Case]);
        assert_eq!(get(0x9da97065)[1], Role::Arg { callee: "FUN_00401000".into(), index: 1, whole: true });
        assert_eq!(get(0x51bff7b1), vec![Role::Arg { callee: "FUN_00401000".into(), index: 2, whole: false }]);
        assert_eq!(get(0xdb41017d)[1], Role::Store { lhs: "* (int *)(param_1 + 0x30)".into() });
        assert_eq!(get(0x7d0b162c), vec![Role::Return]);
        assert!(f.strings.contains("SupportMenu") && f.strings.contains("Enter"), "{:?}", f.strings);
    }

    #[test]
    fn small_values_are_not_sites() {
        let f = scan_function("void f(void){ x = 0x24; y = 0xffff; z = 0x10000; }", 0, &empty(), false);
        assert_eq!(f.sites.iter().map(|(v, _)| *v).collect::<Vec<_>>(), vec![0x10000]);
    }

    #[test]
    fn console_string_addresses_are_strings_not_sites() {
        let mut m = empty();
        m.lo = 0x8200_0000;
        m.hi = 0x8300_0000;
        m.text.insert(0x820d9408, "SupportMenu");
        let src = "void f(void){ uVar3 = 0xffffffff820d9408; FUN_827df878(*param_2,uVar3); if (x == 0xc549813f) {} }";
        let f = scan_function(src, 0x8234fd98, &m, true);
        assert!(f.strings.contains("SupportMenu"));
        assert_eq!(f.sites.iter().map(|(v, _)| *v).collect::<Vec<_>>(), vec![0xc549813f]);
        // the x86 decomp inlines strings: a mapped constant there stays a site
        let f = scan_function(src, 0x8234fd98, &m, false);
        assert_eq!(f.sites.len(), 2);
    }

    #[test]
    fn ps3_toc_loads_resolve_to_strings() {
        let mut m = empty();
        m.lo = 0x10000;
        m.hi = 0x0100_0000;
        m.tocs.insert(0x00587390, 0x0090_0000);
        m.words.insert(0x0090_0000 - 0x7a1c, 0x0080_0000); // the base
        m.words.insert(0x0080_0000 - 0x7c88, 0x0070_0000); // the string pointer
        m.text.insert(0x0070_0000, "Enter");
        let src = r#"void FUN_00587390(long param_1){
  ulong uVar3;
  uVar3 = (ulong)*(uint *)(unaff_r2 + -0x7a1c);
  if (iVar2 == -0x62568f9b) { FUN_008d8d88(*param_2,*(undefined4 *)(uVar3 - 0x7c88)); }
}"#;
        let f = scan_function(src, 0x00587390, &m, true);
        assert!(f.strings.contains("Enter"), "{:?}", f.strings);
        assert_eq!(f.sites.iter().map(|(v, _)| *v).collect::<Vec<_>>(), vec![0x9da97065]);
    }

    #[test]
    fn identifier_addresses_name_data_and_strings() {
        let mut m = empty();
        m.text.insert(0x00450000, "Shell");
        let f = scan_function("void f(void){ g(s_Shell_00450000, &DAT_00460010, FUN_00470000); }", 0, &m, false);
        assert!(f.strings.contains("Shell"));
        assert_eq!(f.addresses.iter().copied().collect::<Vec<_>>(), vec![0x00450000, 0x00460010]);
    }

    #[test]
    fn a_run_of_pool_hashes_at_one_stride_is_a_table_with_its_unknown_slots() {
        let mut pool = NamePool::default();
        let names = ["pause", "loading", "unloading", "shell"];
        let text = format!(
            "{{\"pandemic_hash_m2\":{{{}}}}}",
            names.iter().map(|n| format!("\"0x{:08X}\":[\"{n}\"]", pandemic_hash_m2_bytes(n.as_bytes()))).collect::<Vec<_>>().join(",")
        );
        assert_eq!(pool.add_file(&text, "t").unwrap(), 4);
        let mut m = empty();
        m.lo = 0x0040_0000;
        m.hi = 0x0040_1000;
        // stride 8: hash, pointer, hash, pointer …; slot 2 is an unknown hash-shaped value
        let vals = [pandemic_hash_m2_bytes(b"pause"), pandemic_hash_m2_bytes(b"loading"), 0x53056c27, pandemic_hash_m2_bytes(b"shell")];
        for (k, v) in vals.iter().enumerate() {
            m.words.insert(0x0040_0100 + 8 * k as u32, *v);
            m.words.insert(0x0040_0104 + 8 * k as u32, 0x0040_0800);
        }
        // hash-shaped words beyond the outermost pool hashes are trimmed off the run
        m.words.insert(0x0040_00f8, 0x2004_7f80);
        m.words.insert(0x0040_0120, 0x0001_0001);
        let hits: Vec<u32> = m.data_words().filter(|(_, v)| pool.contains(*v)).map(|(va, _)| va).collect();
        let t = tables(&hits, &pool, &m);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].va, t[0].stride, t[0].slots.len()), (0x0040_0100, 8, 4));
        assert_eq!(t[0].slots[2], (0x0040_0110, 0x53056c27));
    }

    #[test]
    fn about_two_percent_of_hashes_are_structured() {
        let n = 100_000;
        let hit = (0..n).filter(|i| structured(pandemic_hash_m2_bytes(format!("name_{i}").as_bytes()))).count();
        let share = hit as f64 / n as f64;
        assert!((0.01..0.03).contains(&share), "{share}");
    }

    #[test]
    fn a_pool_name_that_does_not_hash_to_its_key_is_an_error() {
        let mut pool = NamePool::default();
        let e = pool.add_file(r#"{"pandemic_hash_m2":{"0x12345678":["pause"]}}"#, "t.json").unwrap_err().to_string();
        assert!(e.contains("1 names do not hash to their key") && e.contains("\"pause\""), "{e}");
        let e = pool.add_file(r#"{"other":{}}"#, "t.json").unwrap_err().to_string();
        assert!(e.contains("no \"pandemic_hash_m2\" object"), "{e}");
    }
}
