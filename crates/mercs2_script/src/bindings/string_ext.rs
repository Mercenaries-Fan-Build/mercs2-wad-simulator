//! `String` engine binding namespace — luaL_Reg table VA 0x00dfda70, 13 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! The 13 names are exactly the JavaScript / ActionScript-2 `String.prototype` surface (`charAt`,
//! `charCodeAt`, `concat`, `indexOf`, `lastIndexOf`, `slice`, `split`, `substr`, `substring`,
//! `toLowerCase`, `toString`, `toUpperCase`, `valueOf`) — this table is the engine's JS-style string
//! polyfill (the game bundles the GFx/Flash8 AS2 runtime, see `scaleform_gfx_class_map`). All 13 are
//! **pure** string ops, computed from arguments with no engine state, so each gets a real body with
//! **0-based, JS-faithful** semantics (the string operated on is the first argument; game strings are
//! ASCII so char = byte). Zero corpus call sites — semantics follow the JS spec exactly.

use mlua::{Lua, MultiValue, Result as LuaResult, Value};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "StringExt";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "String";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00dfda70;

pub const REQUIRED: &[Required] = &[
    Required { name: "charAt", corpus_calls: 0 },
    Required { name: "charCodeAt", corpus_calls: 0 },
    Required { name: "concat", corpus_calls: 0 },
    Required { name: "indexOf", corpus_calls: 0 },
    Required { name: "lastIndexOf", corpus_calls: 0 },
    Required { name: "slice", corpus_calls: 0 },
    Required { name: "split", corpus_calls: 0 },
    Required { name: "substr", corpus_calls: 0 },
    Required { name: "substring", corpus_calls: 0 },
    Required { name: "toLowerCase", corpus_calls: 0 },
    Required { name: "toString", corpus_calls: 0 },
    Required { name: "toUpperCase", corpus_calls: 0 },
    Required { name: "valueOf", corpus_calls: 0 },
];

/// Clamp a possibly-negative JS index against `len` for `slice`/`substring` semantics.
fn clamp_idx(i: i64, len: i64) -> i64 {
    if i < 0 {
        (len + i).max(0)
    } else {
        i.min(len)
    }
}

/// All 13 cfuncs are pure JS-style string ops — every one gets a real body.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // charAt(s, i) -> single-char string at 0-based index i, or "" if out of range.
    b.real(
        "charAt",
        lua.create_function(|_, (s, i): (String, i64)| {
            let bytes = s.as_bytes();
            Ok(if i >= 0 && (i as usize) < bytes.len() {
                (bytes[i as usize] as char).to_string()
            } else {
                String::new()
            })
        })?,
    )?;
    // charCodeAt(s, i) -> numeric code of char at 0-based index i, or -1 if out of range (JS: NaN).
    b.real(
        "charCodeAt",
        lua.create_function(|_, (s, i): (String, i64)| {
            let bytes = s.as_bytes();
            Ok(if i >= 0 && (i as usize) < bytes.len() {
                bytes[i as usize] as i64
            } else {
                -1
            })
        })?,
    )?;
    // concat(s, ...) -> s with every following argument coerced to string and appended.
    b.real(
        "concat",
        lua.create_function(|lua, args: MultiValue| {
            let mut out = String::new();
            for v in args {
                let s = lua.coerce_string(v)?;
                if let Some(s) = s {
                    out.push_str(&s.to_string_lossy());
                }
            }
            Ok(out)
        })?,
    )?;
    // indexOf(s, sub [, from]) -> 0-based index of first occurrence at/after `from`, else -1.
    b.real(
        "indexOf",
        lua.create_function(|_, (s, sub, from): (String, String, Option<i64>)| {
            let start = from.unwrap_or(0).max(0) as usize;
            if start > s.len() {
                return Ok(-1i64);
            }
            Ok(match s[start..].find(&sub) {
                Some(p) => (start + p) as i64,
                None => -1,
            })
        })?,
    )?;
    // lastIndexOf(s, sub) -> 0-based index of last occurrence, else -1.
    b.real(
        "lastIndexOf",
        lua.create_function(|_, (s, sub): (String, String)| {
            Ok(match s.rfind(&sub) {
                Some(p) => p as i64,
                None => -1i64,
            })
        })?,
    )?;
    // slice(s, start [, end]) -> substring; negative indices count from the end (JS String.slice).
    b.real(
        "slice",
        lua.create_function(|_, (s, start, end): (String, i64, Option<i64>)| {
            let len = s.len() as i64;
            let a = clamp_idx(start, len);
            let e = clamp_idx(end.unwrap_or(len), len);
            Ok(if a < e {
                s[a as usize..e as usize].to_string()
            } else {
                String::new()
            })
        })?,
    )?;
    // split(s, sep) -> array (Lua table, 1-based) of pieces. Empty sep -> array of characters.
    b.real(
        "split",
        lua.create_function(|lua, (s, sep): (String, Option<String>)| {
            let t = lua.create_table()?;
            match sep {
                None => {
                    t.set(1, s)?;
                }
                Some(sep) if sep.is_empty() => {
                    for (i, c) in s.chars().enumerate() {
                        t.set(i + 1, c.to_string())?;
                    }
                }
                Some(sep) => {
                    for (i, piece) in s.split(&sep).enumerate() {
                        t.set(i + 1, piece.to_string())?;
                    }
                }
            }
            Ok(t)
        })?,
    )?;
    // substr(s, start [, length]) -> `length` chars from `start` (negative start = from end; JS).
    b.real(
        "substr",
        lua.create_function(|_, (s, start, length): (String, i64, Option<i64>)| {
            let len = s.len() as i64;
            let a = if start < 0 { (len + start).max(0) } else { start.min(len) };
            let n = length.unwrap_or(len - a).max(0);
            let e = (a + n).min(len);
            Ok(if a < e {
                s[a as usize..e as usize].to_string()
            } else {
                String::new()
            })
        })?,
    )?;
    // substring(s, a [, b]) -> chars between indices; negatives clamp to 0 and args swap if a>b (JS).
    b.real(
        "substring",
        lua.create_function(|_, (s, a, b): (String, i64, Option<i64>)| {
            let len = s.len() as i64;
            let mut lo = a.clamp(0, len);
            let mut hi = b.unwrap_or(len).clamp(0, len);
            if lo > hi {
                std::mem::swap(&mut lo, &mut hi);
            }
            Ok(s[lo as usize..hi as usize].to_string())
        })?,
    )?;
    // toLowerCase(s) / toUpperCase(s) -> case-folded copy.
    b.real(
        "toLowerCase",
        lua.create_function(|_, s: String| Ok(s.to_lowercase()))?,
    )?;
    b.real(
        "toUpperCase",
        lua.create_function(|_, s: String| Ok(s.to_uppercase()))?,
    )?;
    // toString(v) / valueOf(v) -> string coercion of the value (valueOf returns the value unchanged in
    // JS, but on the primitive String object both yield the string; game usage is string coercion).
    b.real(
        "toString",
        lua.create_function(|lua, v: Value| {
            Ok(lua
                .coerce_string(v)?
                .map(|s| s.to_string_lossy())
                .unwrap_or_default())
        })?,
    )?;
    b.real(
        "valueOf",
        lua.create_function(|lua, v: Value| {
            Ok(lua
                .coerce_string(v)?
                .map(|s| s.to_string_lossy())
                .unwrap_or_default())
        })?,
    )?;

    b.install_global(GLOBAL)
}
