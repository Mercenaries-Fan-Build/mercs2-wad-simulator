//! Lua values, the handle types, and the Rust ↔ Lua conversion traits.
//!
//! # There is no integer
//!
//! Lua 5.1 has a single numeric type, and in this build it is `f32`. [`Value`] therefore has no
//! `Integer` variant — porting code that matched on one should match [`Value::Number`] instead.
//!
//! This is not a limitation to work around, it is the game's arithmetic. Anything needing more than
//! 24 bits of integer precision must not travel as a number: **handles cross as
//! [`Value::LightUserData`]**, which is lossless for a full pointer and is what retail did (tag 2).

use std::os::raw::{c_char, c_int, c_void};

use super::{Error, Lua, LuaRef, Result};
use crate::sys;

/// A pointer-sized opaque handle. The engine's GUIDs travel this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LightUserData(pub *mut c_void);

/// An interned Lua string. Lua strings are byte strings, not necessarily UTF-8.
#[derive(Clone)]
pub struct LuaString(pub(crate) LuaRef);

impl LuaString {
    /// The bytes, as Lua stores them.
    pub fn as_bytes(&self) -> Vec<u8> {
        // SAFETY: the ref holds a string; `lua_tolstring` on it cannot raise.
        unsafe {
            let l = self.0.lua.state;
            sys::lua_checkstack(l, 1);
            self.0.push();
            let mut len = 0usize;
            let p = sys::lua_tolstring(l, -1, &mut len);
            let out = if p.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p as *const u8, len).to_vec()
            };
            sys::lua_pop(l, 1);
            out
        }
    }

    /// Lossy UTF-8 view — the game's strings are ASCII in practice.
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.as_bytes()).into_owned()
    }
}

impl std::fmt::Debug for LuaString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string_lossy())
    }
}

/// A Lua table.
#[derive(Clone)]
pub struct Table(pub(crate) LuaRef);

impl Table {
    /// `t[key]`, honouring metatables.
    pub fn get<V: FromLua>(&self, key: impl IntoLua) -> Result<V> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::get");
        // SAFETY: table and key are pushed then consumed by `lua_gettable`, leaving one value.
        unsafe {
            lua.reserve(2);
            self.0.push();
            push_value(&lua, &key.into_lua(&lua)?);
            sys::lua_gettable(lua.state(), -2);
            let v = value_from_stack(&lua, -1)?;
            sys::lua_pop(lua.state(), 2);
            V::from_lua(v, &lua)
        }
    }

    /// `t[key] = value`, honouring metatables.
    pub fn set(&self, key: impl IntoLua, value: impl IntoLua) -> Result<()> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::set");
        // SAFETY: table, key and value are pushed then consumed by `lua_settable`.
        unsafe {
            lua.reserve(3);
            self.0.push();
            push_value(&lua, &key.into_lua(&lua)?);
            push_value(&lua, &value.into_lua(&lua)?);
            sys::lua_settable(lua.state(), -3);
            sys::lua_pop(lua.state(), 1);
            Ok(())
        }
    }

    /// `t[key]` ignoring metatables.
    pub fn raw_get<V: FromLua>(&self, key: impl IntoLua) -> Result<V> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::raw_get");
        // SAFETY: as `get`, via `lua_rawget`.
        unsafe {
            lua.reserve(2);
            self.0.push();
            push_value(&lua, &key.into_lua(&lua)?);
            sys::lua_rawget(lua.state(), -2);
            let v = value_from_stack(&lua, -1)?;
            sys::lua_pop(lua.state(), 2);
            V::from_lua(v, &lua)
        }
    }

    /// `t[key] = value` ignoring metatables.
    pub fn raw_set(&self, key: impl IntoLua, value: impl IntoLua) -> Result<()> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::raw_set");
        // SAFETY: as `set`, via `lua_rawset`.
        unsafe {
            lua.reserve(3);
            self.0.push();
            push_value(&lua, &key.into_lua(&lua)?);
            push_value(&lua, &value.into_lua(&lua)?);
            sys::lua_rawset(lua.state(), -3);
            sys::lua_pop(lua.state(), 1);
            Ok(())
        }
    }

    /// Set (or with `None`, clear) this table's metatable.
    ///
    /// The module system runs on this: a module's environment gets `__index → _G` so its misses
    /// fall through to the stdlib, and `inherit(base)` chains `__index → base`.
    pub fn set_metatable(&self, metatable: Option<Table>) -> Result<()> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::set_metatable");
        // SAFETY: `lua_setmetatable` pops the metatable, leaving the table we then pop ourselves.
        unsafe {
            lua.reserve(2);
            self.0.push();
            match metatable {
                Some(mt) => mt.0.push(),
                None => sys::lua_pushnil(lua.state()),
            }
            sys::lua_setmetatable(lua.state(), -2);
            sys::lua_pop(lua.state(), 1);
            Ok(())
        }
    }

    /// This table's metatable, if it has one.
    pub fn get_metatable(&self) -> Option<Table> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::get_metatable");
        // SAFETY: `lua_getmetatable` pushes nothing when there is no metatable.
        unsafe {
            lua.reserve(2);
            self.0.push();
            let has = sys::lua_getmetatable(lua.state(), -1) != 0;
            let out = has.then(|| Table(lua.pop_ref()));
            sys::lua_pop(lua.state(), 1);
            out
        }
    }

    /// The `#t` border — the array part's length.
    pub fn len(&self) -> usize {
        let _balance = super::Balance::new(&self.0.lua(), "Table::len");
        // SAFETY: `lua_objlen` on a table cannot raise.
        unsafe {
            let l = self.0.lua.state;
            sys::lua_checkstack(l, 1);
            self.0.push();
            let n = sys::lua_objlen(l, -1);
            sys::lua_pop(l, 1);
            n
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append to the sequence part — `t[#t + 1] = value`, like `table.insert(t, value)`.
    pub fn push(&self, value: impl IntoLua) -> Result<()> {
        self.raw_set((self.len() + 1) as f64, value)
    }

    /// Remove a key, returning what was there. Metatables are not consulted.
    ///
    /// An integer key inside `1..=#t` shifts the tail down, like `table.remove`; any other key —
    /// including a string — is simply cleared. Both spellings are used: the module loader removes
    /// globals by name, the binding surface removes list entries by index.
    pub fn raw_remove(&self, key: impl IntoLua) -> Result<Value> {
        let lua = self.0.lua();
        let key = key.into_lua(&lua)?;
        let n = self.len();

        // Only an in-range positive integer index is a sequence removal.
        let idx = match &key {
            Value::Number(f) if *f >= 1.0 && f.fract() == 0.0 && (*f as usize) <= n => *f as usize,
            _ => {
                let old = self.raw_get::<Value>(key.clone())?;
                self.raw_set(key, Value::Nil)?;
                return Ok(old);
            }
        };

        let removed = self.raw_get::<Value>(idx as f64)?;
        for j in idx..n {
            let next = self.raw_get::<Value>((j + 1) as f64)?;
            self.raw_set(j as f64, next)?;
        }
        self.raw_set(n as f64, Value::Nil)?;
        Ok(removed)
    }

    /// The array part, `t[1..#t]`, decoded.
    ///
    /// Collected eagerly rather than lazily: an iterator borrowing the Lua stack across user code
    /// is a footgun, and no call site needs the laziness.
    pub fn sequence_values<V: FromLua>(&self) -> std::vec::IntoIter<Result<V>> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::sequence_values");
        let n = self.len();
        let mut out = Vec::with_capacity(n);
        for i in 1..=n {
            // SAFETY: raw geti on a table cannot raise; stack is balanced each iteration.
            let item = unsafe {
                lua.reserve(2);
                self.0.push();
                sys::lua_rawgeti(lua.state(), -1, i as c_int);
                let v = value_from_stack(&lua, -1);
                sys::lua_pop(lua.state(), 2);
                v
            };
            out.push(item.and_then(|v| V::from_lua(v, &lua)));
        }
        out.into_iter()
    }

    /// Every key/value pair, decoded. Order is unspecified, as in Lua.
    pub fn pairs<K: FromLua, V: FromLua>(&self) -> std::vec::IntoIter<Result<(K, V)>> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Table::pairs");
        let l = lua.state();
        let mut out = Vec::new();
        // SAFETY: the standard `lua_next` walk; the stack returns to its base afterwards.
        unsafe {
            lua.reserve(3);
            self.0.push();
            sys::lua_pushnil(l);
            while sys::lua_next(l, -2) != 0 {
                let k = value_from_stack(&lua, -2);
                let v = value_from_stack(&lua, -1);
                out.push(match (k, v) {
                    (Ok(k), Ok(v)) => K::from_lua(k, &lua).and_then(|k| Ok((k, V::from_lua(v, &lua)?))),
                    (Err(e), _) | (_, Err(e)) => Err(e),
                });
                // Pop the value, keep the key for the next step.
                sys::lua_pop(l, 1);
            }
            sys::lua_pop(l, 1);
        }
        out.into_iter()
    }
}

/// Identity, not structural equality: two handles are equal when they name the same table, which
/// is what Lua's own `==` reports for tables without an `__eq` metamethod. The module loader uses it
/// to find which cached module a given table is.
impl PartialEq for Table {
    fn eq(&self, other: &Table) -> bool {
        // SAFETY: both refs push one value; `lua_rawequal` compares without invoking metamethods.
        unsafe {
            let lua = self.0.lua();
            let _balance = super::Balance::new(&lua, "Table::eq");
            lua.reserve(2);
            self.0.push();
            other.0.push();
            let same = sys::lua_rawequal(lua.state(), -1, -2) != 0;
            sys::lua_pop(lua.state(), 2);
            same
        }
    }
}

impl Eq for Table {}

impl std::fmt::Debug for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<table len={}>", self.len())
    }
}

/// A Lua function — either a script function or a Rust binding.
#[derive(Clone)]
pub struct Function(pub(crate) LuaRef);

impl Function {
    /// Call it. Errors from the script come back as [`Error::RuntimeError`] rather than unwinding,
    /// because every call goes through `lua_pcall`.
    pub fn call<R: FromLuaMulti>(&self, args: impl IntoLuaMulti) -> Result<R> {
        let lua = self.0.lua();
        let _balance = super::Balance::new(&lua, "Function::call");
        // SAFETY: the function is pushed and consumed by exactly one pcall.
        unsafe {
            lua.reserve(1);
            self.0.push();
            super::call_on_stack(&lua, args, sys::LUA_MULTRET)
        }
    }
}

impl std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<function>")
    }
}

use super::multi::{FromLuaMulti, IntoLuaMulti};

/// Any Lua value.
///
/// **No `Integer` variant** — see the module docs.
#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Boolean(bool),
    LightUserData(LightUserData),
    Number(sys::lua_Number),
    String(LuaString),
    Table(Table),
    Function(Function),
    /// A value this layer does not model (userdata, thread). Preserved losslessly so it round-trips
    /// through a binding untouched; `.1` is Lua's own type name, for diagnostics.
    Other(Opaque, &'static str),
}

/// An unmodelled Lua value, held by reference so it survives a trip through Rust unchanged.
///
/// There is deliberately no way to inspect one. The layer models what the script host and the game
/// corpus use; anything else must pass through rather than be silently degraded to `nil`.
#[derive(Clone)]
pub struct Opaque(pub(crate) LuaRef);

impl std::fmt::Debug for Opaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<opaque>")
    }
}

impl Value {
    /// Lua's name for this value's type, as `type()` reports it.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::LightUserData(_) => "userdata",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Function(_) => "function",
            Value::Other(_, n) => n,
        }
    }

    /// Lua truthiness: everything except `nil` and `false`.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Boolean(false))
    }

    /// The number, if this is one. No string coercion — use [`Lua::coerce_string`]'s counterpart
    /// `f32::from_lua` when you want Lua's implicit conversion.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The number truncated to an integer, if this is a number.
    ///
    /// Only meaningful up to 2^24 — this VM's numbers are `f32`. Above that the value has already
    /// been rounded by the time it reaches here, so this cannot recover it.
    pub fn as_i64(&self) -> Option<i64> {
        self.as_f32().map(|n| n as i64)
    }

    /// The string contents, if this is a string. Lossy for non-UTF-8, which the game's ASCII
    /// strings never are.
    pub fn as_str(&self) -> Option<String> {
        match self {
            Value::String(s) => Some(s.to_string_lossy()),
            _ => None,
        }
    }

    /// Alias of [`Value::as_str`], for call sites that read better as a conversion.
    pub fn to_str(&self) -> Option<String> {
        self.as_str()
    }
}

impl std::fmt::Debug for LuaRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<ref {}>", self.key)
    }
}

/// Read the value at `idx` without disturbing the stack.
///
/// # Safety
/// `idx` must be a valid stack index in `lua`'s state.
pub(crate) unsafe fn value_from_stack(lua: &Lua, idx: c_int) -> Result<Value> {
    let l = lua.state();
    let t = sys::lua_type(l, idx);
    Ok(match t {
        sys::LUA_TNIL | sys::LUA_TNONE => Value::Nil,
        sys::LUA_TBOOLEAN => Value::Boolean(sys::lua_toboolean(l, idx) != 0),
        sys::LUA_TLIGHTUSERDATA => Value::LightUserData(LightUserData(sys::lua_touserdata(l, idx))),
        sys::LUA_TNUMBER => Value::Number(sys::lua_tonumber(l, idx)),
        sys::LUA_TSTRING => {
            lua.reserve(1);
            sys::lua_pushvalue(l, idx);
            Value::String(LuaString(lua.pop_ref()))
        }
        sys::LUA_TTABLE => {
            lua.reserve(1);
            sys::lua_pushvalue(l, idx);
            Value::Table(Table(lua.pop_ref()))
        }
        sys::LUA_TFUNCTION => {
            lua.reserve(1);
            sys::lua_pushvalue(l, idx);
            Value::Function(Function(lua.pop_ref()))
        }
        other => {
            lua.reserve(1);
            sys::lua_pushvalue(l, idx);
            let name = match other {
                sys::LUA_TUSERDATA => "userdata",
                sys::LUA_TTHREAD => "thread",
                _ => "unknown",
            };
            Value::Other(Opaque(lua.pop_ref()), name)
        }
    })
}

/// Push a value onto the stack.
///
/// # Safety
/// `lua`'s state must have a free stack slot.
pub(crate) unsafe fn push_value(lua: &Lua, v: &Value) {
    let l = lua.state();
    match v {
        Value::Nil => sys::lua_pushnil(l),
        Value::Boolean(b) => sys::lua_pushboolean(l, *b as c_int),
        Value::LightUserData(p) => sys::lua_pushlightuserdata(l, p.0),
        Value::Number(n) => sys::lua_pushnumber(l, *n),
        Value::String(s) => s.0.push(),
        Value::Table(t) => t.0.push(),
        Value::Function(f) => f.0.push(),
        Value::Other(r, _) => r.0.push(),
    }
}

// ─── conversion traits ───────────────────────────────────────────────────────────────────────

/// Decode a Lua value into a Rust type.
pub trait FromLua: Sized {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self>;
}

/// Encode a Rust value as a Lua value.
pub trait IntoLua {
    fn into_lua(self, lua: &Lua) -> Result<Value>;
}

impl FromLua for Value {
    fn from_lua(value: Value, _: &Lua) -> Result<Value> {
        Ok(value)
    }
}

impl IntoLua for Value {
    fn into_lua(self, _: &Lua) -> Result<Value> {
        Ok(self)
    }
}

impl FromLua for bool {
    /// Lua truthiness, not a type check — `if x then` accepts anything, and so do the bindings.
    fn from_lua(value: Value, _: &Lua) -> Result<bool> {
        Ok(value.is_truthy())
    }
}

impl IntoLua for bool {
    fn into_lua(self, _: &Lua) -> Result<Value> {
        Ok(Value::Boolean(self))
    }
}

impl FromLua for String {
    fn from_lua(value: Value, _: &Lua) -> Result<String> {
        match value {
            Value::String(s) => Ok(s.to_string_lossy()),
            // Lua coerces numbers to strings implicitly, and game scripts rely on it.
            Value::Number(n) => Ok(format_number(n)),
            other => Err(Error::conversion(other.type_name(), "String")),
        }
    }
}

impl IntoLua for String {
    fn into_lua(self, lua: &Lua) -> Result<Value> {
        self.as_str().into_lua(lua)
    }
}

impl IntoLua for &str {
    fn into_lua(self, lua: &Lua) -> Result<Value> {
        // SAFETY: `lua_pushlstring` copies the bytes; the slice outlives the call.
        unsafe {
            lua.reserve(1);
            sys::lua_pushlstring(lua.state(), self.as_ptr() as *const c_char, self.len());
            Ok(Value::String(LuaString(lua.pop_ref())))
        }
    }
}

impl FromLua for LuaString {
    fn from_lua(value: Value, _: &Lua) -> Result<LuaString> {
        match value {
            Value::String(s) => Ok(s),
            other => Err(Error::conversion(other.type_name(), "LuaString")),
        }
    }
}

impl IntoLua for LuaString {
    fn into_lua(self, _: &Lua) -> Result<Value> {
        Ok(Value::String(self))
    }
}

impl FromLua for Table {
    fn from_lua(value: Value, _: &Lua) -> Result<Table> {
        match value {
            Value::Table(t) => Ok(t),
            other => Err(Error::conversion(other.type_name(), "Table")),
        }
    }
}

impl IntoLua for Table {
    fn into_lua(self, _: &Lua) -> Result<Value> {
        Ok(Value::Table(self))
    }
}

impl FromLua for Function {
    fn from_lua(value: Value, _: &Lua) -> Result<Function> {
        match value {
            Value::Function(f) => Ok(f),
            other => Err(Error::conversion(other.type_name(), "Function")),
        }
    }
}

impl IntoLua for Function {
    fn into_lua(self, _: &Lua) -> Result<Value> {
        Ok(Value::Function(self))
    }
}

impl FromLua for LightUserData {
    fn from_lua(value: Value, _: &Lua) -> Result<LightUserData> {
        match value {
            Value::LightUserData(p) => Ok(p),
            Value::Nil => Ok(LightUserData(std::ptr::null_mut())),
            other => Err(Error::conversion(other.type_name(), "LightUserData")),
        }
    }
}

impl IntoLua for LightUserData {
    fn into_lua(self, _: &Lua) -> Result<Value> {
        Ok(Value::LightUserData(self))
    }
}

/// `Option<T>` is how a binding accepts an absent argument — `nil` and "not passed" both decode to
/// `None`, which is what the game's Lua expects of its own engine calls.
impl<T: FromLua> FromLua for Option<T> {
    fn from_lua(value: Value, lua: &Lua) -> Result<Option<T>> {
        match value {
            Value::Nil => Ok(None),
            v => Ok(Some(T::from_lua(v, lua)?)),
        }
    }
}

impl<T: IntoLua> IntoLua for Option<T> {
    fn into_lua(self, lua: &Lua) -> Result<Value> {
        match self {
            Some(v) => v.into_lua(lua),
            None => Ok(Value::Nil),
        }
    }
}

impl<T: IntoLua> IntoLua for Vec<T> {
    fn into_lua(self, lua: &Lua) -> Result<Value> {
        let t = lua.create_table()?;
        for (i, v) in self.into_iter().enumerate() {
            t.raw_set((i + 1) as f64, v)?;
        }
        Ok(Value::Table(t))
    }
}

impl<T: FromLua> FromLua for Vec<T> {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Vec<T>> {
        match value {
            Value::Table(t) => t.sequence_values::<T>().collect(),
            other => Err(Error::conversion(other.type_name(), "Vec")),
        }
    }
}

/// Numeric conversions.
///
/// Every one of these goes through `f32`, because that is the only numeric type this VM has. The
/// integer impls exist for ergonomics at the binding boundary — they are **not** a wide-integer
/// channel, and anything above 2^24 will not survive. Handles use [`LightUserData`].
macro_rules! impl_number {
    ($($t:ty),*) => {$(
        impl FromLua for $t {
            fn from_lua(value: Value, _: &Lua) -> Result<$t> {
                match value {
                    Value::Number(n) => Ok(n as $t),
                    // Lua coerces numeric strings, and the corpus passes them.
                    Value::String(s) => s
                        .to_string_lossy()
                        .trim()
                        .parse::<f64>()
                        .map(|n| n as $t)
                        .map_err(|_| Error::conversion("string", stringify!($t))),
                    other => Err(Error::conversion(other.type_name(), stringify!($t))),
                }
            }
        }

        impl IntoLua for $t {
            fn into_lua(self, _: &Lua) -> Result<Value> {
                Ok(Value::Number(self as sys::lua_Number))
            }
        }
    )*};
}

impl_number!(i8, u8, i16, u16, i32, u32, i64, u64, isize, usize, f32, f64);

/// Render a number the way Lua's `tostring` does under `LUA_NUMBER_FMT "%.7g"`, so a number
/// coerced to a string in Rust matches one coerced inside the VM.
fn format_number(n: sys::lua_Number) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n:.7}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}
