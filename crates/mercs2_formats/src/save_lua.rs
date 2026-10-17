//! Lua AST + parser + serializer for Mercenaries 2 `.profile` payloads.
//!
//! The decompressed save payload is a `return { … }` Lua chunk (plain source text,
//! not bytecode — the engine `loadstring`s it; see
//! [`crate::save::parse_save_state`] module docs and
//! `docs/reverse_engineer/save_serialize_code_map.md` §2.1). To edit a save
//! safely you need a round-tripping parser: text-level splicing cannot preserve
//! the subtle structural invariants (sibling comma, nested-brace balance, string
//! escape fidelity) that the engine's own `loadstring` relies on — the first
//! save-rollback attempt corrupted its output exactly this way, and the game
//! fell back to a fresh-start path.
//!
//! This module parses the game's dialect — a subset of Lua 5.1 syntax:
//!   - literals `nil` / `true` / `false`
//!   - numbers (integer, float, negative, `0x…` hex) stored as `f64`
//!   - strings `"…"` / `'…'` with Lua escape sequences (`\n`, `\t`, `\\`,
//!     `\"`, `\'`, `\r`, `\0`, `\xNN`, decimal `\NNN`)
//!   - table constructors `{ … }` with `[key] = value` entries, `name = value`
//!     shorthand entries, positional entries, both `,` and `;` separators, and a
//!     trailing separator
//!   - function calls `Qualified.Name(args…)` as opaque values (the saves use
//!     `Sys.StringToGuid('0x…')` inside `tCollected` arrays)
//!   - line comments `-- …` and long comments `--[[ … ]]`
//!
//! Not supported (deliberately — the saves never contain these):
//!   - operators / expressions beyond literals / calls
//!   - unquoted identifiers as values (variables)
//!   - multi-return function calls
//!
//! If the parser hits one, it errors with the exact byte offset so a save that
//! strays from the expected shape surfaces loudly instead of silently.
//!
//! Round-trips byte-exactly: `parse → serialize → parse` yields the same AST as
//! `parse` for every vendored retail save (`crate::game_paths::SAVE_FIXTURES`).
//! Serialized output is accepted by the retail engine's `loadstring` — the tests
//! verify this via `mercs2_luac` (the exact Lua 5.1.5 reference) in the integration
//! test suite.

use std::collections::BTreeMap;
use std::fmt;

/// A parsed Lua value.
///
/// Numbers are kept as `f64` because the game serializes every number as a float
/// (`[1.000000]`); the parser preserves the original textual form in
/// [`Number::source`] so the serializer round-trips bit-for-bit.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(Number),
    String(LuaString),
    Table(Table),
    /// Function call used as a value, e.g. `Sys.StringToGuid('0x0013E2C6')`.
    /// Kept opaque — the args are themselves `Value`s, but the callee name is a
    /// dotted path that the game resolves at `loadstring`-time.
    Call(Call),
}

/// A Lua number, with its original textual form preserved for round-trip fidelity.
#[derive(Debug, Clone)]
pub struct Number {
    /// Parsed `f64` value — what consumers compare against.
    pub value: f64,
    /// The exact substring the parser consumed, e.g. `"1.000000"` or `"0x1F"`.
    /// Serializers prefer this when the same `Number` is written back; synthetic
    /// numbers (constructed by code) have `None` and get formatted on write.
    pub source: Option<String>,
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        // Compare by numeric value — a parse-serialize-parse round trip may lose
        // the textual source without changing meaning.
        self.value.to_bits() == other.value.to_bits()
    }
}

impl Number {
    /// Build a float from a Rust `f64`, formatting it as the game does
    /// (`"<n>.000000"` for integers in float form) when serialized.
    pub fn from_f64(v: f64) -> Self {
        Self {
            value: v,
            source: None,
        }
    }

    /// Build an integer-form number (e.g. for `tMyFlowData` counts the game writes
    /// as `1.000000`, but the keys occasionally shown as pure integers in flow bodies).
    pub fn index(n: usize) -> Self {
        Self {
            value: n as f64,
            source: Some(format!("{n}.000000")),
        }
    }
}

/// A Lua string literal, carrying its content and the quote style used in source.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaString {
    pub value: String,
    /// `'"'` or `'\''` — the style we re-emit with when round-tripping. New
    /// strings default to double-quote.
    pub quote: char,
}

impl LuaString {
    pub fn new(s: impl Into<String>) -> Self {
        Self {
            value: s.into(),
            quote: '"',
        }
    }
}

/// A function-call value.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// Dotted qualified name, e.g. `"Sys.StringToGuid"`.
    pub func: String,
    pub args: Vec<Value>,
}

/// An ordered Lua table.
///
/// Preserves insertion order — critical for the save because `ipairs` iteration
/// stops at the first nil and because `tCulledBindings` is a dense 1-based array.
/// Keys may be any Value in principle; in the retail saves they are either strings
/// or numbers (floats as `[1.000000]`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub entries: Vec<Entry>,
}

/// One `[key] = value,` / `name = value,` / `value,` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `Some(k)` for a keyed entry, `None` for a positional entry (implicit next
    /// integer key). The retail saves always use keyed entries.
    pub key: Option<Value>,
    pub value: Value,
    /// Original source form of the key — `Keyed::Bracketed("[\"foo\"]")` for the
    /// game's long form, `Keyed::Shorthand("foo")` for `name = …` sugar, `None`
    /// for a positional entry. The serializer prefers this when present so
    /// unmodified entries round-trip byte-exactly.
    pub key_form: Option<KeyForm>,
}

/// How the key was written in source.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyForm {
    /// `[expr]` — the explicit bracketed form the saves use. The inner `expr` is
    /// in `Entry::key`.
    Bracketed,
    /// `name` — the shorthand `name = value` sugar. Only valid when `Entry::key`
    /// is `Value::String { value, .. }` matching an identifier.
    Shorthand,
}

impl Entry {
    /// Build a keyed entry with the game's `["key"] = value` style.
    pub fn keyed(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: Some(Value::String(LuaString::new(key))),
            value,
            key_form: Some(KeyForm::Bracketed),
        }
    }

    /// Build a positional entry (used for `tCulledBindings[n]` arrays after we
    /// renumber). The key is a float literal matching the game's `[1.000000]`
    /// style.
    pub fn index(n: usize, value: Value) -> Self {
        Self {
            key: Some(Value::Number(Number::index(n))),
            value,
            key_form: Some(KeyForm::Bracketed),
        }
    }
}

// ---------------------------------------------------------------------------
// Table navigation / mutation helpers
// ---------------------------------------------------------------------------

impl Table {
    /// Look up by string key. `None` if no entry has that string key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| matches!(&e.key, Some(Value::String(s)) if s.value == key))
            .map(|e| &e.value)
    }

    /// Mutable string-key lookup.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|e| matches!(&e.key, Some(Value::String(s)) if s.value == key))
            .map(|e| &mut e.value)
    }

    /// Remove by string key. Returns the removed value if present.
    pub fn remove_str(&mut self, key: &str) -> Option<Value> {
        let pos = self.entries.iter().position(
            |e| matches!(&e.key, Some(Value::String(s)) if s.value == key),
        )?;
        Some(self.entries.remove(pos).value)
    }

    /// Iterate string-keyed entries in insertion order as `(key, value)`.
    pub fn iter_str(&self) -> impl Iterator<Item = (&str, &Value)> + '_ {
        self.entries.iter().filter_map(|e| match &e.key {
            Some(Value::String(s)) => Some((s.value.as_str(), &e.value)),
            _ => None,
        })
    }

    /// Iterate ALL entries (string or numeric keyed, positional) in insertion order.
    pub fn iter_entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        self.entries.iter()
    }

    /// Retain only the entries matching the predicate; others are dropped.
    /// Returns the number of entries removed.
    pub fn retain(&mut self, mut keep: impl FnMut(&Entry) -> bool) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| keep(e));
        before - self.entries.len()
    }

    /// Renumber this table's keys as a dense 1-based float array (`[1.000000]`,
    /// `[2.000000]`, …), the format the game writes `tCulledBindings` and
    /// `tLayerData` in. Non-array entries are left alone — only entries whose
    /// current key is already a number (or positional) are renumbered. Returns
    /// how many entries were renumbered.
    pub fn renumber_as_array(&mut self) -> usize {
        let mut n = 0usize;
        let mut next = 1usize;
        for e in &mut self.entries {
            let is_numeric_or_positional =
                e.key.is_none() || matches!(&e.key, Some(Value::Number(_)));
            if is_numeric_or_positional {
                e.key = Some(Value::Number(Number::index(next)));
                e.key_form = Some(KeyForm::Bracketed);
                next += 1;
                n += 1;
            }
        }
        n
    }

    /// Count string-keyed entries. Useful for diagnostics.
    pub fn len_str_keys(&self) -> usize {
        self.iter_str().count()
    }

    /// Collect string-keyed entries into a BTreeMap — convenient for assertions.
    pub fn to_str_map(&self) -> BTreeMap<String, Value> {
        self.iter_str()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }
}

impl Value {
    /// Convenience: expect a table, else error.
    pub fn as_table(&self) -> Result<&Table, Error> {
        match self {
            Value::Table(t) => Ok(t),
            other => Err(Error::new_simple(format!(
                "expected table, got {}",
                other.kind()
            ))),
        }
    }

    /// Convenience: expect a mutable table.
    pub fn as_table_mut(&mut self) -> Result<&mut Table, Error> {
        match self {
            Value::Table(t) => Ok(t),
            other => Err(Error::new_simple(format!(
                "expected table, got {}",
                other.kind()
            ))),
        }
    }

    /// A short label for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Call(_) => "call",
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A parse error with the exact byte offset where it occurred.
#[derive(Debug, Clone)]
pub struct Error {
    pub message: String,
    pub offset: Option<usize>,
}

impl Error {
    fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
        }
    }
    fn new_simple(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(o) = self.offset {
            write!(f, "save_lua parse error at byte {o}: {}", self.message)
        } else {
            write!(f, "save_lua error: {}", self.message)
        }
    }
}

impl std::error::Error for Error {}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a `return { … }` chunk (the inflated save payload) into a [`Value`].
///
/// Accepts a leading `return` keyword. Returns `Err` with an exact byte offset
/// on anything the dialect doesn't cover (see module docs).
pub fn parse_chunk(text: &str) -> Result<Value, Error> {
    let mut p = Parser::new(text);
    p.skip_ws_and_comments();
    // The game writes `return {...}` at the top; be tolerant of either form.
    if p.consume_keyword("return") {
        p.skip_ws_and_comments();
    }
    let v = p.parse_value()?;
    p.skip_ws_and_comments();
    if p.pos < p.src.len() {
        return Err(Error::new(
            format!("trailing input after top-level value: {:?}", p.peek_debug()),
            p.pos,
        ));
    }
    Ok(v)
}

/// Parse a bare value (no `return` prefix). For tests and sub-tree use.
pub fn parse_value(text: &str) -> Result<Value, Error> {
    let mut p = Parser::new(text);
    p.skip_ws_and_comments();
    let v = p.parse_value()?;
    p.skip_ws_and_comments();
    if p.pos < p.src.len() {
        return Err(Error::new(
            format!("trailing input after value: {:?}", p.peek_debug()),
            p.pos,
        ));
    }
    Ok(v)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            src: text.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn peek_debug(&self) -> String {
        let slice =
            &self.src[self.pos..self.src.len().min(self.pos + 24)];
        String::from_utf8_lossy(slice).into_owned()
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while let Some(c) = self.peek() {
                if c.is_ascii_whitespace() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.peek() == Some(b'-') && self.peek_at(1) == Some(b'-') {
                self.pos += 2;
                if self.peek() == Some(b'[') && self.peek_at(1) == Some(b'[') {
                    self.pos += 2;
                    while self.pos + 1 < self.src.len() {
                        if self.src[self.pos] == b']' && self.src[self.pos + 1] == b']' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                } else {
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == b'\n' {
                            break;
                        }
                    }
                }
                continue;
            }
            break;
        }
    }

    fn consume_keyword(&mut self, kw: &str) -> bool {
        let bytes = kw.as_bytes();
        if self.src[self.pos..].starts_with(bytes) {
            let after = self.pos + bytes.len();
            let next = self.src.get(after).copied();
            if next.is_none() || !is_ident_cont(next.unwrap()) {
                self.pos = after;
                return true;
            }
        }
        false
    }

    fn parse_value(&mut self) -> Result<Value, Error> {
        self.skip_ws_and_comments();
        let start = self.pos;
        match self.peek() {
            Some(b'{') => Ok(Value::Table(self.parse_table()?)),
            Some(b'"') | Some(b'\'') => Ok(Value::String(self.parse_string()?)),
            Some(c) if c == b'-' || c == b'+' || c.is_ascii_digit() => {
                Ok(Value::Number(self.parse_number()?))
            }
            Some(c) if is_ident_start(c) => {
                let name = self.read_identifier()?;
                match name.as_str() {
                    "nil" => Ok(Value::Nil),
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    _ => {
                        // Qualified name — expect a function call.
                        self.parse_qualified_call(name, start)
                    }
                }
            }
            Some(c) => Err(Error::new(
                format!("unexpected byte 0x{c:02x} ('{}')", c as char),
                self.pos,
            )),
            None => Err(Error::new("unexpected end of input", self.pos)),
        }
    }

    fn parse_qualified_call(
        &mut self,
        first: String,
        _start: usize,
    ) -> Result<Value, Error> {
        let mut func = first;
        while self.peek() == Some(b'.') {
            self.pos += 1;
            let next = self.read_identifier()?;
            func.push('.');
            func.push_str(&next);
        }
        self.skip_ws_and_comments();
        if self.peek() != Some(b'(') {
            return Err(Error::new(
                format!("expected '(' after qualified name `{func}`"),
                self.pos,
            ));
        }
        self.pos += 1;
        let mut args = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some(b')') {
                self.pos += 1;
                break;
            }
            let v = self.parse_value()?;
            args.push(v);
            self.skip_ws_and_comments();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b')') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    return Err(Error::new(
                        format!(
                            "expected ',' or ')' in call to `{func}`, got '{}'",
                            c as char
                        ),
                        self.pos,
                    ));
                }
                None => {
                    return Err(Error::new("unexpected EOF in call", self.pos));
                }
            }
        }
        Ok(Value::Call(Call { func, args }))
    }

    fn read_identifier(&mut self) -> Result<String, Error> {
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Err(Error::new("expected identifier", self.pos));
        };
        if !is_ident_start(c) {
            return Err(Error::new(
                format!("expected identifier, got '{}'", c as char),
                self.pos,
            ));
        }
        self.pos += 1;
        while let Some(c) = self.peek() {
            if is_ident_cont(c) {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
    }

    fn parse_number(&mut self) -> Result<Number, Error> {
        let start = self.pos;
        if self.peek() == Some(b'+') || self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if self.peek() == Some(b'0')
            && (self.peek_at(1) == Some(b'x') || self.peek_at(1) == Some(b'X'))
        {
            self.pos += 2;
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let s = std::str::from_utf8(&self.src[start..self.pos])
                .map_err(|_| Error::new("invalid hex number", start))?;
            let hex_part = s.trim_start_matches(['+', '-']).trim_start_matches("0x");
            let n = u64::from_str_radix(hex_part, 16)
                .map_err(|e| Error::new(format!("hex parse: {e}"), start))?;
            let v = if s.starts_with('-') { -(n as f64) } else { n as f64 };
            return Ok(Number {
                value: v,
                source: Some(s.to_string()),
            });
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        if self.peek() == Some(b'e') || self.peek() == Some(b'E') {
            self.pos += 1;
            if self.peek() == Some(b'+') || self.peek() == Some(b'-') {
                self.pos += 1;
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.pos])
            .map_err(|_| Error::new("invalid number encoding", start))?;
        let v: f64 = s
            .parse()
            .map_err(|e| Error::new(format!("number parse: {e}"), start))?;
        Ok(Number {
            value: v,
            source: Some(s.to_string()),
        })
    }

    fn parse_string(&mut self) -> Result<LuaString, Error> {
        let quote = self.peek().unwrap();
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(Error::new("unterminated string", self.pos)),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(LuaString {
                        value: out,
                        quote: quote as char,
                    });
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let esc = self
                        .peek()
                        .ok_or_else(|| Error::new("unterminated escape", self.pos))?;
                    self.pos += 1;
                    match esc {
                        b'a' => out.push('\x07'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0C'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'v' => out.push('\x0B'),
                        b'\\' => out.push('\\'),
                        b'"' => out.push('"'),
                        b'\'' => out.push('\''),
                        b'\n' => out.push('\n'),
                        b'x' => {
                            let hi = self
                                .bump()
                                .ok_or_else(|| Error::new("truncated \\x escape", self.pos))?;
                            let lo = self
                                .bump()
                                .ok_or_else(|| Error::new("truncated \\x escape", self.pos))?;
                            let b = hex_pair(hi, lo)
                                .ok_or_else(|| Error::new("bad \\x hex", self.pos))?;
                            out.push(b as char);
                        }
                        c if c.is_ascii_digit() => {
                            let mut v = (c - b'0') as u32;
                            for _ in 0..2 {
                                if let Some(nc) = self.peek() {
                                    if nc.is_ascii_digit() {
                                        self.pos += 1;
                                        v = v * 10 + (nc - b'0') as u32;
                                    } else {
                                        break;
                                    }
                                }
                            }
                            if v > 255 {
                                return Err(Error::new(
                                    format!("decimal escape {v} out of range"),
                                    self.pos,
                                ));
                            }
                            out.push(v as u8 as char);
                        }
                        other => {
                            return Err(Error::new(
                                format!("unknown escape '\\{}'", other as char),
                                self.pos,
                            ));
                        }
                    }
                }
                Some(c) => {
                    self.pos += 1;
                    out.push(c as char);
                }
            }
        }
    }

    fn parse_table(&mut self) -> Result<Table, Error> {
        assert_eq!(self.peek(), Some(b'{'));
        self.pos += 1;
        let mut entries = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Table { entries });
                }
                None => return Err(Error::new("unterminated table", self.pos)),
                _ => {}
            }
            let entry = self.parse_entry()?;
            entries.push(entry);
            self.skip_ws_and_comments();
            match self.peek() {
                Some(b',') | Some(b';') => {
                    self.pos += 1;
                }
                Some(b'}') => {}
                Some(c) => {
                    return Err(Error::new(
                        format!(
                            "expected ',' or '}}' between table entries, got '{}'",
                            c as char
                        ),
                        self.pos,
                    ));
                }
                None => return Err(Error::new("unterminated table", self.pos)),
            }
        }
    }

    fn parse_entry(&mut self) -> Result<Entry, Error> {
        self.skip_ws_and_comments();
        match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                let key = self.parse_value()?;
                self.skip_ws_and_comments();
                if self.peek() != Some(b']') {
                    return Err(Error::new("expected ']' after bracketed key", self.pos));
                }
                self.pos += 1;
                self.skip_ws_and_comments();
                if self.peek() != Some(b'=') {
                    return Err(Error::new(
                        "expected '=' after '[k]' bracketed key",
                        self.pos,
                    ));
                }
                self.pos += 1;
                self.skip_ws_and_comments();
                let value = self.parse_value()?;
                Ok(Entry {
                    key: Some(key),
                    value,
                    key_form: Some(KeyForm::Bracketed),
                })
            }
            Some(c) if is_ident_start(c) => {
                // Shorthand `name = value` OR a bare value like `nil` / `true` /
                // `Qualified.Call(…)`. Look ahead: an `=` after the identifier
                // (possibly a qualified name) means shorthand key; otherwise it's
                // a positional value.
                let save = self.pos;
                let name = self.read_identifier()?;
                let after_name = self.pos;
                self.skip_ws_and_comments();
                let next = self.peek();
                if next == Some(b'=') {
                    self.pos += 1;
                    self.skip_ws_and_comments();
                    let value = self.parse_value()?;
                    Ok(Entry {
                        key: Some(Value::String(LuaString {
                            value: name,
                            quote: '"',
                        })),
                        value,
                        key_form: Some(KeyForm::Shorthand),
                    })
                } else {
                    // Rewind — this was actually a value, parse from scratch.
                    self.pos = save;
                    let value = self.parse_value()?;
                    // Prevent the warning: after_name was captured but is unused in
                    // this branch — kept so the shorthand/positional distinction
                    // reads clearly in the diff.
                    let _ = after_name;
                    Ok(Entry {
                        key: None,
                        value,
                        key_form: None,
                    })
                }
            }
            _ => {
                let value = self.parse_value()?;
                Ok(Entry {
                    key: None,
                    value,
                    key_form: None,
                })
            }
        }
    }
}

fn is_ident_start(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphabetic()
}
fn is_ident_cont(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphanumeric()
}
fn hex_pair(hi: u8, lo: u8) -> Option<u8> {
    let h = hex_digit(hi)?;
    let l = hex_digit(lo)?;
    Some((h << 4) | l)
}
fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(10 + c - b'a'),
        b'A'..=b'F' => Some(10 + c - b'A'),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

/// Options for the serializer.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Emit a leading `return ` keyword before the top-level value.
    ///
    /// **The default is `false` — matching the on-disk form of a `.profile`.**
    /// The retail engine prepends `"return "` at load time (`save_serialize_code_map.md`
    /// §2.1 point 3; verified on every vendored save: the inflated zlib payload
    /// starts with `{`, not `return`). Writing one on-disk makes the engine's
    /// final `loadstring` input `"return return { … }"` — a syntax error that
    /// silently yields `tSaveData = nil` and sends the player back to the
    /// VzaCon001 fresh-start intro.
    ///
    /// Set this to `true` only when serializing for a context that itself does
    /// `loadstring` on the raw text WITHOUT a return-wrapper (not the retail
    /// engine's save path).
    pub emit_return: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self { emit_return: false }
    }
}

/// Serialize a [`Value`] back to Lua source.
///
/// Matches the game's writer style:
///   - each table entry on its own line, with a trailing comma
///   - string keys as `["name"]` (never shorthand), unless the entry's
///     `key_form` was `Shorthand` originally
///   - numbers reuse their original textual source when available
///   - strings use their original quote style, double-quote for synthetic strings
pub fn serialize_with(value: &Value, opt: SerializeOptions) -> String {
    let mut out = String::new();
    if opt.emit_return {
        out.push_str("return ");
    }
    write_value(&mut out, value, 0);
    out
}

/// Serialize with default options (emits `return` prefix).
pub fn serialize(value: &Value) -> String {
    serialize_with(value, SerializeOptions::default())
}

fn write_value(out: &mut String, v: &Value, _indent: usize) {
    match v {
        Value::Nil => out.push_str("nil"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => write_number(out, n),
        Value::String(s) => write_string(out, s),
        Value::Table(t) => write_table(out, t),
        Value::Call(c) => write_call(out, c),
    }
}

fn write_number(out: &mut String, n: &Number) {
    if let Some(s) = &n.source {
        out.push_str(s);
    } else if n.value.is_finite() && n.value.fract() == 0.0 && n.value.abs() < 1e15 {
        // Match the game's float form for integers: `1.000000`.
        out.push_str(&format!("{:.6}", n.value));
    } else {
        out.push_str(&format!("{}", n.value));
    }
}

fn write_string(out: &mut String, s: &LuaString) {
    let q = s.quote;
    out.push(q);
    for c in s.value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' if q == '"' => out.push_str("\\\""),
            '\'' if q == '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\{}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(q);
}

fn write_table(out: &mut String, t: &Table) {
    out.push('{');
    if !t.entries.is_empty() {
        out.push('\n');
        for e in &t.entries {
            write_entry(out, e);
            out.push_str(",\n");
        }
    }
    out.push('}');
}

fn write_entry(out: &mut String, e: &Entry) {
    match (&e.key, &e.key_form) {
        (Some(Value::String(s)), Some(KeyForm::Shorthand)) if is_valid_ident(&s.value) => {
            out.push_str(&s.value);
            out.push_str(" = ");
        }
        (Some(k), _) => {
            out.push('[');
            write_value(out, k, 0);
            out.push_str("] = ");
        }
        (None, _) => {}
    }
    write_value(out, &e.value, 0);
}

fn write_call(out: &mut String, c: &Call) {
    out.push_str(&c.func);
    out.push('(');
    for (i, a) in c.args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_value(out, a, 0);
    }
    out.push(')');
}

fn is_valid_ident(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || !is_ident_start(b[0]) {
        return false;
    }
    b[1..].iter().all(|c| is_ident_cont(*c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_literals() {
        assert_eq!(parse_value("nil").unwrap(), Value::Nil);
        assert_eq!(parse_value("true").unwrap(), Value::Bool(true));
        assert_eq!(parse_value("false").unwrap(), Value::Bool(false));
        if let Value::Number(n) = parse_value("1.000000").unwrap() {
            assert_eq!(n.value, 1.0);
            assert_eq!(n.source.as_deref(), Some("1.000000"));
        } else {
            panic!();
        }
        if let Value::Number(n) = parse_value("-3.5").unwrap() {
            assert_eq!(n.value, -3.5);
        } else {
            panic!();
        }
        if let Value::Number(n) = parse_value("0x1F").unwrap() {
            assert_eq!(n.value, 31.0);
            assert_eq!(n.source.as_deref(), Some("0x1F"));
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_strings_with_escapes() {
        let v = parse_value(r#""[vehicle.wz10]""#).unwrap();
        assert_eq!(
            v,
            Value::String(LuaString {
                value: "[vehicle.wz10]".into(),
                quote: '"'
            })
        );
        let v = parse_value(r#""a\"b\\c\n""#).unwrap();
        if let Value::String(s) = v {
            assert_eq!(s.value, "a\"b\\c\n");
        } else {
            panic!();
        }
        // Single-quoted (the saves use `Sys.StringToGuid('0x001234')`).
        let v = parse_value(r#"'0x0013E2C6'"#).unwrap();
        if let Value::String(s) = v {
            assert_eq!(s.value, "0x0013E2C6");
            assert_eq!(s.quote, '\'');
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_table_with_bracketed_and_positional_entries() {
        let v = parse_value(r#"{
["a"] = 1.0,
["b"] = "two",
[1.000000] = "first",
"positional",
}"#)
        .unwrap();
        let t = v.as_table().unwrap();
        assert_eq!(t.entries.len(), 4);
        assert_eq!(t.get("a").unwrap(), &Value::Number(Number::from_f64(1.0)));
        assert_eq!(
            t.get("b").unwrap(),
            &Value::String(LuaString {
                value: "two".into(),
                quote: '"'
            })
        );
        // The positional entry retains None key.
        assert!(t.entries.last().unwrap().key.is_none());
    }

    #[test]
    fn parse_qualified_function_call_as_value() {
        let v = parse_value(r#"Sys.StringToGuid('0x0013E2C6')"#).unwrap();
        if let Value::Call(c) = v {
            assert_eq!(c.func, "Sys.StringToGuid");
            assert_eq!(c.args.len(), 1);
            if let Value::String(s) = &c.args[0] {
                assert_eq!(s.value, "0x0013E2C6");
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_chunk_accepts_leading_return() {
        let v = parse_chunk(r#"return { ["x"] = 1.0 }"#).unwrap();
        let t = v.as_table().unwrap();
        assert_eq!(t.len_str_keys(), 1);
    }

    #[test]
    fn parse_and_serialize_round_trip_simple() {
        let src = r#"return {
["tFlowData"] = {
["tCulledBindings"] = {
[1.000000] = "Start",
[2.000000] = "VzaCon001",
},
["tMyFlowData"] = {
["VzaCon001"] = 1.000000,
["PmcCon001"] = 1.000000,
},
["tActiveMissions"] = {
["PmcJob001"] = {
["nState"] = 1.000000,
["tCollected"] = {
Sys.StringToGuid('0x0013E2C6'),
},
},
},
},
}"#;
        let v = parse_chunk(src).unwrap();
        let out = serialize(&v);
        // Second parse must equal the first — the acid test of a round-trip.
        let v2 = parse_chunk(&out).unwrap();
        assert_eq!(v, v2, "parse(serialize(parse(x))) == parse(x)");
    }

    #[test]
    fn renumber_as_array_renumbers_only_numeric_keys() {
        let mut t = Table {
            entries: vec![
                Entry::index(1, Value::String(LuaString::new("a"))),
                Entry::index(2, Value::String(LuaString::new("b"))),
                Entry::keyed("name", Value::String(LuaString::new("alpha"))),
                Entry::index(3, Value::String(LuaString::new("c"))),
            ],
        };
        // Remove the middle string entry — simulating a filter. Two of the three
        // numeric-keyed entries remain, plus the one shorthand-keyed entry.
        t.retain(|e| {
            !matches!(&e.value, Value::String(s) if s.value == "b")
        });
        let n = t.renumber_as_array();
        assert_eq!(n, 2, "two numeric entries survive the retain and get renumbered");
        // After renumber the two remaining numeric entries are 1 and 2.
        let indices: Vec<f64> = t
            .entries
            .iter()
            .filter_map(|e| match &e.key {
                Some(Value::Number(n)) => Some(n.value),
                _ => None,
            })
            .collect();
        assert_eq!(indices, vec![1.0, 2.0]);
        // The string-keyed entry is untouched.
        assert!(t.get("name").is_some());
    }

    #[test]
    fn error_offset_is_reported() {
        let err = parse_value("{ [1.0] = @ }").unwrap_err();
        assert!(err.offset.unwrap() >= 10, "{err}");
    }

    /// Every vendored retail save: inflated payload is a bare `{ … }` body
    /// (NOT a `return { … }` chunk), parses as a value, serializes without a
    /// return prefix, and round-trips.
    ///
    /// This encodes the on-disk invariant that `save_rollback` needs: the engine
    /// prepends `"return "` at load time, so writing one on-disk makes the final
    /// `loadstring` input `"return return { … }"`, a syntax error that silently
    /// yields `tSaveData = nil` and plays the fresh-start intro. If the
    /// vendored retail saves ever violated this invariant, the test here would
    /// fail loudly instead of letting save_rollback ship broken.
    #[test]
    fn round_trip_every_retail_save() {
        use crate::game_paths::{save_fixtures, SAVE_FIXTURES};
        let dir = save_fixtures();
        for name in SAVE_FIXTURES {
            let bytes =
                std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
            let p = crate::save::parse(&bytes).unwrap_or_else(|e| panic!("parse {name}: {e}"));
            let lua = p
                .decompress_lua()
                .unwrap_or_else(|e| panic!("inflate {name}: {e}"));
            let text = std::str::from_utf8(&lua)
                .unwrap_or_else(|e| panic!("utf8 {name}: {e}"));

            // The on-disk invariant: inflated content is a bare `{` (not `return {`).
            let trimmed = text.trim_start();
            assert!(
                trimmed.starts_with('{'),
                "{name}: inflated save payload must start with '{{', got {:?}",
                &trimmed[..trimmed.len().min(16)]
            );
            assert!(
                !trimmed.starts_with("return"),
                "{name}: inflated save payload must NOT carry a `return` prefix — \
                 the engine prepends it at load time; a stored one would produce \
                 `return return {{ … }}` when wrapped",
            );

            // Parse the on-disk form (no `return` wrapper) and round-trip through
            // our serializer in the same mode.
            let v1 = parse_value(text).unwrap_or_else(|e| panic!("parse_value {name}: {e}"));
            let out = serialize_with(&v1, SerializeOptions { emit_return: false });
            assert!(
                out.trim_start().starts_with('{'),
                "{name}: serialize_with(emit_return=false) must produce bare table body"
            );
            let v2 = parse_value(&out)
                .unwrap_or_else(|e| panic!("re-parse of serialize({name}): {e}"));
            assert_eq!(
                v1, v2,
                "{name}: parse → serialize → re-parse must round-trip"
            );
        }
    }
}
