//! Multiple values — Lua's calling convention, and the tuple plumbing behind
//! `move |_, (a, b): (Guid, f32)|`.

use super::{FromLua, IntoLua, Lua, Result, Value};

/// An ordered run of Lua values: a call's arguments, or its results.
#[derive(Clone, Debug, Default)]
pub struct MultiValue(Vec<Value>);

impl MultiValue {
    pub fn new() -> MultiValue {
        MultiValue(Vec::new())
    }

    /// Build from values already in call order.
    pub fn from_vec(v: Vec<Value>) -> MultiValue {
        MultiValue(v)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Positional read; out of range is [`Value::Nil`], matching Lua's own treatment of a missing
    /// argument.
    pub fn get(&self, i: usize) -> Value {
        self.0.get(i).cloned().unwrap_or(Value::Nil)
    }

    /// Take the front value, or `nil` when exhausted — the decoding primitive for tuples.
    pub fn pop_front(&mut self) -> Value {
        if self.0.is_empty() {
            Value::Nil
        } else {
            self.0.remove(0)
        }
    }

    pub fn push(&mut self, v: Value) {
        self.0.push(v)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        self.0.iter()
    }

    /// Consume into the underlying values.
    pub fn into_vec(self) -> Vec<Value> {
        self.0
    }
}

impl IntoIterator for MultiValue {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MultiValue {
    type Item = &'a Value;
    type IntoIter = std::slice::Iter<'a, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<Value> for MultiValue {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> MultiValue {
        MultiValue(iter.into_iter().collect())
    }
}

/// A homogeneous variable-length argument list.
///
/// The script host uses this to forward a caller-supplied argument vector plus the loaded module
/// into a continuation — `fCallback(unpack(tArgs), mModule)` — which is the shape `dynamic_import`
/// drives the whole task-instantiation chain through.
#[derive(Clone, Debug, Default)]
pub struct Variadic<T>(pub Vec<T>);

impl<T> Variadic<T> {
    pub fn new() -> Variadic<T> {
        Variadic(Vec::new())
    }
}

impl<T> FromIterator<T> for Variadic<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Variadic<T> {
        Variadic(iter.into_iter().collect())
    }
}

impl<T> IntoIterator for Variadic<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T> std::ops::Deref for Variadic<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.0
    }
}

/// Decode a call's argument list into a Rust type.
pub trait FromLuaMulti: Sized {
    fn from_lua_multi(values: MultiValue, lua: &Lua) -> Result<Self>;
}

/// Encode a Rust value as a call's return values.
pub trait IntoLuaMulti {
    fn into_lua_multi(self, lua: &Lua) -> Result<MultiValue>;
}

impl FromLuaMulti for MultiValue {
    fn from_lua_multi(values: MultiValue, _: &Lua) -> Result<MultiValue> {
        Ok(values)
    }
}

impl IntoLuaMulti for MultiValue {
    fn into_lua_multi(self, _: &Lua) -> Result<MultiValue> {
        Ok(self)
    }
}

impl FromLuaMulti for () {
    /// Extra arguments are ignored, exactly as a Lua function ignores them.
    fn from_lua_multi(_: MultiValue, _: &Lua) -> Result<()> {
        Ok(())
    }
}

impl IntoLuaMulti for () {
    fn into_lua_multi(self, _: &Lua) -> Result<MultiValue> {
        Ok(MultiValue::new())
    }
}

impl<T: FromLua> FromLuaMulti for Variadic<T> {
    fn from_lua_multi(values: MultiValue, lua: &Lua) -> Result<Variadic<T>> {
        values
            .into_iter()
            .map(|v| T::from_lua(v, lua))
            .collect::<Result<Vec<_>>>()
            .map(Variadic)
    }
}

impl<T: IntoLua> IntoLuaMulti for Variadic<T> {
    fn into_lua_multi(self, lua: &Lua) -> Result<MultiValue> {
        self.0
            .into_iter()
            .map(|v| v.into_lua(lua))
            .collect::<Result<Vec<_>>>()
            .map(MultiValue::from_vec)
    }
}

/// A bare value is a one-element list — what makes `Ok(true)` a valid binding return and
/// `|_, name: String|` a valid binding argument.
///
/// This wants to be a blanket `impl<T: FromLua> FromLuaMulti for T`, but that would overlap the
/// concrete impls above (`()`, `MultiValue`, `Variadic<T>`, the tuples) and Rust rejects it. So the
/// single-value case is enumerated instead. The list is every type a binding actually takes or
/// returns; a missing one shows up as a clear "trait not satisfied" at the call site, not as
/// wrong behaviour. Foreign types (the script host's `Guid`) implement the pair themselves.
macro_rules! impl_single_for {
    ($($t:ty),* $(,)?) => {$(
        impl FromLuaMulti for $t {
            fn from_lua_multi(mut values: MultiValue, lua: &Lua) -> Result<$t> {
                <$t as FromLua>::from_lua(values.pop_front(), lua)
            }
        }

        impl IntoLuaMulti for $t {
            fn into_lua_multi(self, lua: &Lua) -> Result<MultiValue> {
                Ok(MultiValue::from_vec(vec![<$t as IntoLua>::into_lua(self, lua)?]))
            }
        }
    )*};
}

impl_single_for!(
    bool, String, f32, f64, i8, u8, i16, u16, i32, u32, i64, u64, isize, usize,
    Value, super::Table, super::Function, super::LuaString, super::LightUserData,
);

impl<T: FromLua> FromLuaMulti for Option<T> {
    fn from_lua_multi(mut values: MultiValue, lua: &Lua) -> Result<Option<T>> {
        <Option<T> as FromLua>::from_lua(values.pop_front(), lua)
    }
}

impl<T: IntoLua> IntoLuaMulti for Option<T> {
    fn into_lua_multi(self, lua: &Lua) -> Result<MultiValue> {
        Ok(MultiValue::from_vec(vec![<Option<T> as IntoLua>::into_lua(self, lua)?]))
    }
}

impl<T: IntoLua> IntoLuaMulti for Vec<T> {
    /// A `Vec` returned from a binding becomes one Lua **table**, not N values — returning a list
    /// of results uses a tuple or [`Variadic`].
    fn into_lua_multi(self, lua: &Lua) -> Result<MultiValue> {
        Ok(MultiValue::from_vec(vec![<Vec<T> as IntoLua>::into_lua(self, lua)?]))
    }
}

impl<T: FromLua> FromLuaMulti for Vec<T> {
    /// The mirror of the above: one returned **table** decodes to a `Vec`, which is how
    /// `sh.eval::<Vec<String>>("return _hits")` reads a list-returning binding. A call that returns
    /// several separate values is a tuple or a [`Variadic`], not a `Vec`.
    fn from_lua_multi(mut values: MultiValue, lua: &Lua) -> Result<Vec<T>> {
        <Vec<T> as FromLua>::from_lua(values.pop_front(), lua)
    }
}

/// Tuples, for the multi-argument bindings.
///
/// The **last element is `FromLuaMulti`, not `FromLua`** — that is what lets a tuple end in a
/// catch-all. `(Guid, String, MultiValue)` reads two typed arguments and absorbs however many
/// remain, which is how the engine's variadic bindings are written (`Vo.Cue` takes a speaker, a cue
/// and a tail of subtitle arguments it ignores). A tuple of plain `FromLua` elements would silently
/// drop that tail instead.
///
/// The same asymmetry on the way out lets a binding return `(bool, MultiValue)` — one flag plus a
/// forwarded result list — rather than nesting them into a table.
macro_rules! impl_tuple {
    ($($name:ident),* ; $last:ident) => {
        // `mut` is unused in the 1-tuple case, which has no head elements to pop.
        #[allow(non_snake_case, unused_mut)]
        impl<$($name: FromLua,)* $last: FromLuaMulti> FromLuaMulti for ($($name,)* $last,) {
            fn from_lua_multi(mut values: MultiValue, lua: &Lua) -> Result<($($name,)* $last,)> {
                // Front-to-back, so a missing trailing argument arrives as `nil` and an
                // `Option<T>` parameter sees `None`.
                $(let $name = <$name as FromLua>::from_lua(values.pop_front(), lua)?;)*
                let $last = <$last as FromLuaMulti>::from_lua_multi(values, lua)?;
                Ok(($($name,)* $last,))
            }
        }

        #[allow(non_snake_case)]
        impl<$($name: IntoLua,)* $last: IntoLuaMulti> IntoLuaMulti for ($($name,)* $last,) {
            fn into_lua_multi(self, lua: &Lua) -> Result<MultiValue> {
                let ($($name,)* $last,) = self;
                let mut out = Vec::new();
                $(out.push(<$name as IntoLua>::into_lua($name, lua)?);)*
                out.extend(<$last as IntoLuaMulti>::into_lua_multi($last, lua)?);
                Ok(MultiValue::from_vec(out))
            }
        }
    };
}

// Ten is not headroom — `Hud.MinimapAddObjective` genuinely takes ten. Twelve leaves room.
impl_tuple!( ; A);
impl_tuple!(A ; B);
impl_tuple!(A, B ; C);
impl_tuple!(A, B, C ; D);
impl_tuple!(A, B, C, D ; E);
impl_tuple!(A, B, C, D, E ; F);
impl_tuple!(A, B, C, D, E, F ; G);
impl_tuple!(A, B, C, D, E, F, G ; H);
impl_tuple!(A, B, C, D, E, F, G, H ; I);
impl_tuple!(A, B, C, D, E, F, G, H, I ; J);
impl_tuple!(A, B, C, D, E, F, G, H, I, J ; K);
impl_tuple!(A, B, C, D, E, F, G, H, I, J, K ; L);
