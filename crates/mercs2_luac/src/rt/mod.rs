//! A safe Rust binding for the vendored VM — what `mlua` would be if it could describe this Lua.
//!
//! It cannot: `mlua-sys` hardcodes `lua_Number = c_double`, and this VM is built with
//! `LUA_NUMBER float` because that is what the game shipped. Binding mlua here would push 8 bytes
//! everywhere the VM reads 4. So the workspace owns this layer, and in exchange gets **one** Lua —
//! the compiler ([`crate::compile`]) and the engine's script host share a single library instead of
//! colliding over unprefixed `lua_*` symbols.
//!
//! The API deliberately mirrors mlua's shape ([`Lua::create_function`], [`Table::get`],
//! [`Function::call`], [`FromLua`]/[`IntoLua`]) so the script host ports by changing its imports
//! rather than its logic.
//!
//! # What is deliberately absent
//!
//! No coroutines, no user-facing userdata, no async, no `RegistryKey`. Measured: the script host
//! uses none of them, and the 122k-line game corpus contains zero `coroutine.*` and zero `debug.*`.
//! An unused wrapper is maintenance; an unused *Lua* feature costs nothing and stays available
//! through [`crate::sys`].
//!
//! # Two hazards this module owns
//!
//! * **`longjmp`.** Lua 5.1 raises errors by unwinding with `longjmp`, which does not run Rust
//!   destructors. Every call that can raise goes through [`sys::lua_pcall`], and the raising path
//!   in [`trampoline`] drops its Rust values *before* calling [`sys::lua_error`].
//! * **Panics.** Unwinding into C is undefined behaviour. [`trampoline`] and the `__gc` hook both
//!   wrap Rust work in `catch_unwind` and convert a panic into a Lua error.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::{Rc, Weak};

use crate::sys::{self, lua_State};

mod multi;
mod value;

pub use multi::{FromLuaMulti, IntoLuaMulti, MultiValue, Variadic};
pub use value::{FromLua, Function, IntoLua, LightUserData, LuaString, Opaque, Table, Value};

/// Why a Lua operation failed.
///
/// Variant names follow mlua's so the script host's `Error::RuntimeError(..)` construction sites
/// port unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A script raised, or a binding returned `Err`.
    RuntimeError(String),
    /// `luaL_loadbuffer` rejected the source.
    SyntaxError(String),
    /// The allocator refused.
    MemoryError(String),
    /// A Lua value could not become the requested Rust type.
    FromLuaConversionError {
        from: &'static str,
        to: &'static str,
        message: Option<String>,
    },
}

impl Error {
    /// Convenience for the common "wrong Lua type" case.
    pub fn conversion(from: &'static str, to: &'static str) -> Error {
        Error::FromLuaConversionError { from, to, message: None }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::RuntimeError(m) => write!(f, "runtime error: {m}"),
            Error::SyntaxError(m) => write!(f, "syntax error: {m}"),
            Error::MemoryError(m) => write!(f, "memory error: {m}"),
            Error::FromLuaConversionError { from, to, message } => match message {
                Some(msg) => write!(f, "cannot convert {from} to {to}: {msg}"),
                None => write!(f, "cannot convert {from} to {to}"),
            },
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

// ─── the state ───────────────────────────────────────────────────────────────────────────────

/// Owns the `lua_State`. Every handle keeps one of these alive, so a [`Table`] outliving the
/// [`Lua`] it came from is safe rather than a dangling pointer.
pub(crate) struct LuaInner {
    pub(crate) state: *mut lua_State,
    /// Set while a callback is executing, so `Drop` on a borrowed view never closes the state.
    borrowed: Cell<bool>,
    /// Type-keyed side storage for state shared between binding namespaces. Held as `Rc<dyn Any>`
    /// so a handle can outlive the borrow of the map.
    app_data: RefCell<HashMap<std::any::TypeId, Rc<dyn std::any::Any>>>,
}

/// A handle to a value stored with [`Lua::set_app_data`]. Derefs to the value.
pub struct AppDataRef<T>(Rc<T>);

impl<T> std::ops::Deref for AppDataRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl Drop for LuaInner {
    fn drop(&mut self) {
        if !self.borrowed.get() {
            // SAFETY: we created this state and this is the last owner.
            unsafe { sys::lua_close(self.state) }
        }
    }
}

/// A Lua interpreter.
///
/// Cheap to clone — every clone refers to the same VM, and the state closes when the last handle
/// (including any [`Table`] or [`Function`]) goes away.
#[derive(Clone)]
pub struct Lua {
    pub(crate) inner: Rc<LuaInner>,
}

impl Lua {
    /// Create a VM with the complete 5.1 standard library open.
    ///
    /// `luaL_openlibs` includes `debug`, `io` and `os`. That is deliberate and matches the original
    /// engine: this host runs trusted decompiled game Lua, and the corpus itself calls `os.*` (22
    /// sites) and `io.*` (19).
    pub fn new() -> Result<Lua> {
        // SAFETY: the state is checked before use and owned by the LuaInner we build from it.
        let state = unsafe { sys::luaL_newstate() };
        if state.is_null() {
            return Err(Error::MemoryError("could not create a Lua state".into()));
        }
        // SAFETY: `state` is a fresh, live state.
        unsafe { sys::luaL_openlibs(state) };
        Ok(Lua {
            inner: Rc::new(LuaInner {
                state,
                borrowed: Cell::new(false),
                app_data: RefCell::new(HashMap::new()),
            }),
        })
    }

    pub(crate) fn state(&self) -> *mut lua_State {
        self.inner.state
    }

    /// The globals table (`LUA_GLOBALSINDEX`).
    pub fn globals(&self) -> Table {
        let _balance = Balance::new(self, "Lua::globals");
        // SAFETY: pushing a pseudo-index is always valid; `pop_ref` takes the value we just pushed.
        unsafe {
            self.reserve(1);
            sys::lua_pushvalue(self.state(), sys::LUA_GLOBALSINDEX);
            Table(self.pop_ref())
        }
    }

    /// A new empty table.
    pub fn create_table(&self) -> Result<Table> {
        let _balance = Balance::new(self, "Lua::create_table");
        // SAFETY: stack space reserved; `lua_newtable` leaves exactly one value.
        unsafe {
            self.reserve(1);
            sys::lua_newtable(self.state());
            Ok(Table(self.pop_ref()))
        }
    }

    /// Intern a Lua string. Takes bytes, because Lua strings are byte strings.
    pub fn create_string(&self, s: impl AsRef<[u8]>) -> Result<LuaString> {
        let _balance = Balance::new(self, "Lua::create_string");
        let b = s.as_ref();
        // SAFETY: `lua_pushlstring` copies; the slice outlives the call.
        unsafe {
            self.reserve(1);
            sys::lua_pushlstring(self.state(), b.as_ptr() as *const c_char, b.len());
            Ok(LuaString(self.pop_ref()))
        }
    }

    /// Wrap a Rust closure as a Lua function.
    ///
    /// `A` is decoded from the call arguments and `R` is encoded as the return values, so a binding
    /// reads `move |_, (a, b): (Guid, f32)| Ok(..)` exactly as it did under mlua.
    pub fn create_function<A, R, F>(&self, f: F) -> Result<Function>
    where
        A: FromLuaMulti,
        R: IntoLuaMulti,
        F: Fn(&Lua, A) -> Result<R> + 'static,
    {
        let _balance = Balance::new(self, "Lua::create_function");
        let weak = Rc::downgrade(&self.inner);
        let boxed: Box<Callback> = Box::new(move |lua, args| {
            let a = A::from_lua_multi(args, lua)?;
            f(lua, a)?.into_lua_multi(lua)
        });
        // SAFETY: stack space reserved; `push_callback` leaves exactly one closure value.
        unsafe {
            self.reserve(2);
            push_callback(self.state(), CallbackBox { lua: weak, f: boxed })?;
            Ok(Function(self.pop_ref()))
        }
    }

    /// A table pre-sized for `narr` array slots and `nrec` hash slots.
    pub fn create_table_with_capacity(&self, narr: usize, nrec: usize) -> Result<Table> {
        let _balance = Balance::new(self, "Lua::create_table_with_capacity");
        // SAFETY: stack space reserved; `lua_createtable` leaves exactly one value.
        unsafe {
            self.reserve(1);
            sys::lua_createtable(self.state(), narr as c_int, nrec as c_int);
            Ok(Table(self.pop_ref()))
        }
    }

    /// Build a sequence table (`t[1..n]`) from an iterator — the shape every `Get*List` binding
    /// hands back to Lua.
    pub fn create_sequence_from<T, I>(&self, iter: I) -> Result<Table>
    where
        T: IntoLua,
        I: IntoIterator<Item = T>,
    {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let t = self.create_table_with_capacity(lower, 0)?;
        for (i, v) in iter.enumerate() {
            t.raw_set((i + 1) as f64, v)?;
        }
        Ok(t)
    }

    /// Lua's own `tostring` coercion: strings pass through, numbers render, everything else is
    /// `None`. Matches the VM's implicit conversion rather than a Rust-side guess at it.
    pub fn coerce_string(&self, v: Value) -> Result<Option<LuaString>> {
        match v {
            Value::String(s) => Ok(Some(s)),
            Value::Number(_) => {
                let rendered = String::from_lua(v, self)?;
                Ok(Some(self.create_string(rendered)?))
            }
            _ => Ok(None),
        }
    }

    /// Attach a value to the VM, keyed by its type.
    ///
    /// The bindings use this for state that several namespaces share — the event manager is
    /// registered by `Event.*` and then fired into by `Object.Kill` and the engine tick. Storing it
    /// on the `Lua` rather than in each closure is what keeps those one object instead of three.
    ///
    /// Replaces any previous value of the same type.
    pub fn set_app_data<T: 'static>(&self, data: T) {
        self.inner
            .app_data
            .borrow_mut()
            .insert(std::any::TypeId::of::<T>(), Rc::new(data));
    }

    /// Retrieve app data by type, or `None` if nothing of that type was set.
    pub fn app_data_ref<T: 'static>(&self) -> Option<AppDataRef<T>> {
        let map = self.inner.app_data.borrow();
        let any = map.get(&std::any::TypeId::of::<T>())?.clone();
        any.downcast::<T>().ok().map(AppDataRef)
    }

    /// Begin loading a chunk. Configure it, then [`Chunk::exec`] or [`Chunk::call`].
    pub fn load<'a>(&'a self, source: impl AsRef<[u8]>) -> Chunk<'a> {
        Chunk { lua: self, source: source.as_ref().to_vec(), name: None, env: None }
    }

    /// Ensure `n` free stack slots, growing the stack if needed.
    ///
    /// # Safety
    /// `self` must hold a live state.
    pub(crate) unsafe fn reserve(&self, n: c_int) {
        if sys::lua_checkstack(self.state(), n) == 0 {
            // The only failure is a stack that cannot grow, which is unrecoverable here.
            panic!("Lua stack could not grow by {n}");
        }
    }

    /// Move the stack-top value into the registry and return a handle to it.
    ///
    /// # Safety
    /// A value must be on the stack top.
    pub(crate) unsafe fn pop_ref(&self) -> LuaRef {
        let key = sys::luaL_ref(self.state(), sys::LUA_REGISTRYINDEX);
        LuaRef { lua: self.inner.clone(), key }
    }

    /// Read the error at the stack top, pop it, and classify by the `lua_pcall` status.
    ///
    /// # Safety
    /// An error value must be on the stack top and `status` must be non-zero.
    unsafe fn pop_error(&self, status: c_int) -> Error {
        let l = self.state();
        let mut len: usize = 0;
        let p = sys::lua_tolstring(l, -1, &mut len);
        let msg = if p.is_null() {
            "unknown error (non-string error value)".to_string()
        } else {
            String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len)).into_owned()
        };
        sys::lua_pop(l, 1);
        match status {
            sys::LUA_ERRSYNTAX => Error::SyntaxError(msg),
            sys::LUA_ERRMEM => Error::MemoryError(msg),
            _ => Error::RuntimeError(msg),
        }
    }

    /// Build a borrowed view for a callback, whose `Drop` must not close the state.
    ///
    /// # Safety
    /// Only valid while `inner`'s state is executing a call.
    unsafe fn borrowed(inner: Rc<LuaInner>) -> BorrowedLua {
        inner.borrowed.set(true);
        BorrowedLua { lua: Lua { inner } }
    }
}

/// A [`Lua`] view handed to a callback. Restores the borrow flag when the call returns.
struct BorrowedLua {
    lua: Lua,
}

impl Drop for BorrowedLua {
    fn drop(&mut self) {
        self.lua.inner.borrowed.set(false);
    }
}

// ─── stack balance ───────────────────────────────────────────────────────────────────────────

/// Asserts, in debug builds, that an operation left the stack exactly as it found it.
///
/// A layer that returns correct answers while leaking one stack slot per call passes every unit
/// test and then dies hours into a session with `stack overflow`. That failure is near-impossible
/// to attribute after the fact, so each entry point declares its balance up front and the VM
/// itself checks it. Compiles to nothing in release.
pub(crate) struct Balance {
    #[cfg(debug_assertions)]
    state: *mut lua_State,
    #[cfg(debug_assertions)]
    base: c_int,
    #[cfg(debug_assertions)]
    what: &'static str,
}

impl Balance {
    /// Record the current depth. `what` names the operation in the failure message.
    #[inline]
    pub(crate) fn new(lua: &Lua, what: &'static str) -> Balance {
        #[cfg(debug_assertions)]
        {
            // SAFETY: reading the stack depth of a live state cannot raise.
            let base = unsafe { sys::lua_gettop(lua.state()) };
            Balance { state: lua.state(), base, what }
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = (lua, what);
            Balance {}
        }
    }
}

#[cfg(debug_assertions)]
impl Drop for Balance {
    fn drop(&mut self) {
        // A `longjmp` skips this entirely, and an in-flight panic must not be masked by a second
        // one — asserting during unwind turns a readable failure into an abort.
        if std::thread::panicking() {
            return;
        }
        // SAFETY: the state outlives this guard; every holder is a borrow of a live `Lua`.
        let now = unsafe { sys::lua_gettop(self.state) };
        assert_eq!(
            now, self.base,
            "`{}` left the Lua stack at {} but found it at {} — a leaked slot here becomes a \
             stack overflow after enough calls",
            self.what, now, self.base
        );
    }
}

// ─── references ──────────────────────────────────────────────────────────────────────────────

/// A Lua value parked in the registry so Rust can hold it across calls.
pub(crate) struct LuaRef {
    pub(crate) lua: Rc<LuaInner>,
    pub(crate) key: c_int,
}

impl LuaRef {
    /// Push the referenced value onto the stack.
    ///
    /// # Safety
    /// The state must be live and have a free stack slot.
    pub(crate) unsafe fn push(&self) {
        sys::lua_rawgeti(self.lua.state, sys::LUA_REGISTRYINDEX, self.key);
    }

    pub(crate) fn lua(&self) -> Lua {
        Lua { inner: self.lua.clone() }
    }
}

impl Clone for LuaRef {
    fn clone(&self) -> LuaRef {
        // A registry key is not refcounted, so a clone needs its own entry.
        // SAFETY: pushing an existing ref and re-reffing it leaves the stack balanced.
        unsafe {
            let l = self.lua.state;
            sys::lua_checkstack(l, 1);
            self.push();
            let key = sys::luaL_ref(l, sys::LUA_REGISTRYINDEX);
            LuaRef { lua: self.lua.clone(), key }
        }
    }
}

impl Drop for LuaRef {
    fn drop(&mut self) {
        // SAFETY: `key` was produced by `luaL_ref` on this state and is released exactly once.
        unsafe { sys::luaL_unref(self.lua.state, sys::LUA_REGISTRYINDEX, self.key) }
    }
}

// ─── the C boundary ──────────────────────────────────────────────────────────────────────────

type Callback = dyn Fn(&Lua, MultiValue) -> Result<MultiValue>;

/// What a Rust-backed Lua function stores in its upvalue.
///
/// The [`Weak`] is what keeps this from being a reference cycle: the closure lives inside the VM,
/// so a strong handle would keep the VM alive forever.
struct CallbackBox {
    lua: Weak<LuaInner>,
    f: Box<Callback>,
}

/// Metatable name for the callback userdata, carrying the `__gc` that drops the boxed closure.
const CALLBACK_MT: &[u8] = b"mercs2_luac.callback\0";

/// Push a Rust closure as a Lua C-closure with the boxed callback as upvalue 1.
///
/// # Safety
/// `l` must be a live state with two free stack slots.
unsafe fn push_callback(l: *mut lua_State, cb: CallbackBox) -> Result<()> {
    let ud = sys::lua_newuserdata(l, std::mem::size_of::<CallbackBox>()) as *mut CallbackBox;
    if ud.is_null() {
        return Err(Error::MemoryError("could not allocate a callback".into()));
    }
    std::ptr::write(ud, cb);

    // `luaL_newmetatable` returns non-zero the first time, which is when the `__gc` goes on.
    if sys::luaL_newmetatable(l, CALLBACK_MT.as_ptr() as *const c_char) != 0 {
        sys::lua_pushcfunction(l, callback_gc);
        sys::lua_setfield(l, -2, b"__gc\0".as_ptr() as *const c_char);
    }
    sys::lua_setmetatable(l, -2);
    sys::lua_pushcclosure(l, trampoline, 1);
    Ok(())
}

/// Drop the boxed closure when Lua collects its userdata.
///
/// A closure's own destructor could panic; that must not unwind into the collector.
unsafe extern "C-unwind" fn callback_gc(l: *mut lua_State) -> c_int {
    let ud = sys::lua_touserdata(l, 1) as *mut CallbackBox;
    if !ud.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| std::ptr::drop_in_place(ud)));
    }
    0
}

/// The one function every Rust binding is called through.
///
/// Structure is load-bearing: all Rust work happens inside `catch_unwind`, and every raising path
/// leaves that scope — so no Rust destructor is pending when [`sys::lua_error`] `longjmp`s past us.
unsafe extern "C-unwind" fn trampoline(l: *mut lua_State) -> c_int {
    let outcome: std::result::Result<Result<c_int>, _> =
        catch_unwind(AssertUnwindSafe(|| -> Result<c_int> {
            let ud = sys::lua_touserdata(l, sys::lua_upvalueindex(1)) as *mut CallbackBox;
            if ud.is_null() {
                return Err(Error::RuntimeError("binding lost its callback upvalue".into()));
            }
            let Some(inner) = (*ud).lua.upgrade() else {
                return Err(Error::RuntimeError("binding outlived its Lua state".into()));
            };
            let borrowed = Lua::borrowed(inner);
            let lua = &borrowed.lua;

            // Collect the arguments, clearing the stack so returns start from a clean base.
            let n = sys::lua_gettop(l);
            let mut args = Vec::with_capacity(n as usize);
            for i in 1..=n {
                args.push(value::value_from_stack(lua, i)?);
            }
            sys::lua_settop(l, 0);

            let rets = ((*ud).f)(lua, MultiValue::from_vec(args))?;

            let count = rets.len() as c_int;
            lua.reserve(count.max(1));
            for v in rets {
                value::push_value(lua, &v);
            }
            Ok(count)
        }));

    // Reduce to a plain String in its own statement, so the `Error` / panic payload — both of
    // which own heap memory — are dropped HERE, before anything can `longjmp`. A destructor still
    // pending when `lua_error` jumps would never run, and under a forced unwind that is UB, not
    // merely a leak.
    let msg = match outcome {
        Ok(Ok(n)) => return n,
        Ok(Err(e)) => e.to_string(),
        Err(panic) => {
            let detail = if let Some(s) = panic.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown payload".to_string()
            };
            format!("panic in a Rust binding: {detail}")
        }
    };
    raise(l, msg)
}

/// Push `msg` and raise it, having dropped every Rust value first.
///
/// # Safety
/// `l` must be live with a free stack slot. Never returns — `lua_error` `longjmp`s.
unsafe fn raise(l: *mut lua_State, msg: String) -> c_int {
    // `lua_pushlstring` copies, so the String can go before the jump. Scoping it here is what
    // makes the `lua_error` below run with no pending Rust destructor.
    {
        let m = msg;
        sys::lua_checkstack(l, 1);
        sys::lua_pushlstring(l, m.as_ptr() as *const c_char, m.len());
    }
    sys::lua_error(l)
}

// ─── chunks ──────────────────────────────────────────────────────────────────────────────────

/// A chunk being prepared for loading. Mirrors mlua's builder so call sites port unchanged.
pub struct Chunk<'a> {
    lua: &'a Lua,
    source: Vec<u8>,
    name: Option<CString>,
    env: Option<Table>,
}

impl<'a> Chunk<'a> {
    /// Name shown in errors and tracebacks. A leading `@` marks it as a file name, `=` as a
    /// literal — Lua's own convention, passed through untouched.
    pub fn set_name(mut self, name: impl AsRef<str>) -> Chunk<'a> {
        self.name = CString::new(name.as_ref()).ok();
        self
    }

    /// Run the chunk with `env` as its function environment.
    ///
    /// This is `lua_setfenv`, the native 5.1 primitive the original engine's `_SYS._IMPORT` used —
    /// not a shim over 5.4's `_ENV` upvalue.
    pub fn set_environment(mut self, env: Table) -> Chunk<'a> {
        self.env = Some(env);
        self
    }

    /// Load the chunk, leaving the function on the stack.
    ///
    /// # Safety
    /// Caller owns the pushed function and must consume or pop it.
    unsafe fn load_onto_stack(&self) -> Result<()> {
        let l = self.lua.state();
        self.lua.reserve(2);
        let default = CString::new("=(load)").expect("static name");
        let name = self.name.as_ref().unwrap_or(&default);
        let status = sys::luaL_loadbuffer(
            l,
            self.source.as_ptr() as *const c_char,
            self.source.len(),
            name.as_ptr(),
        );
        if status != sys::LUA_OK {
            return Err(self.lua.pop_error(status));
        }
        if let Some(env) = &self.env {
            env.0.push();
            if sys::lua_setfenv(l, -2) == 0 {
                sys::lua_pop(l, 1);
                return Err(Error::RuntimeError(
                    "lua_setfenv refused the chunk (not a function?)".into(),
                ));
            }
        }
        Ok(())
    }

    /// Load without running, yielding the chunk as a callable.
    ///
    /// `luaL_loadbuffer` dispatches on the `\x1bLua` signature, so the source may be **either** Lua
    /// text or a precompiled LuaQ chunk. That is what lets this host load the bytecode retail
    /// shipped in `scripts_vz` directly — our `lundump` reads the game's 32-bit/float dialect, so
    /// its chunks are not foreign to us.
    pub fn into_function(self) -> Result<Function> {
        // SAFETY: the loaded function is the only value left on the stack, and `pop_ref` takes it.
        unsafe {
            let _balance = Balance::new(self.lua, "Chunk::into_function");
            self.load_onto_stack()?;
            Ok(Function(self.lua.pop_ref()))
        }
    }

    /// Run for side effects.
    pub fn exec(self) -> Result<()> {
        self.call::<()>(())
    }

    /// Run and decode the results.
    pub fn call<R: FromLuaMulti>(self, args: impl IntoLuaMulti) -> Result<R> {
        let _balance = Balance::new(self.lua, "Chunk::call");
        // SAFETY: the chunk is loaded and consumed by exactly one pcall.
        unsafe {
            self.load_onto_stack()?;
            let lua = self.lua;
            call_on_stack(lua, args, sys::LUA_MULTRET)
        }
    }

    /// Run and decode a single result. Same as [`Chunk::call`] with no arguments.
    pub fn eval<R: FromLuaMulti>(self) -> Result<R> {
        self.call(())
    }
}

/// `pcall` a function already on the stack top, pushing `args` after it.
///
/// # Safety
/// A callable must be on the stack top.
pub(crate) unsafe fn call_on_stack<R: FromLuaMulti>(
    lua: &Lua,
    args: impl IntoLuaMulti,
    nresults: c_int,
) -> Result<R> {
    let l = lua.state();
    let vals = args.into_lua_multi(lua)?;
    let nargs = vals.len() as c_int;
    lua.reserve(nargs + 1);
    for v in &vals {
        value::push_value(lua, v);
    }

    // Base of the results, once the function and its arguments are consumed.
    let base = sys::lua_gettop(l) - nargs - 1;
    let status = sys::lua_pcall(l, nargs, nresults, 0);
    if status != sys::LUA_OK {
        return Err(lua.pop_error(status));
    }

    let produced = sys::lua_gettop(l) - base;
    let mut out = Vec::with_capacity(produced.max(0) as usize);
    for i in 0..produced {
        out.push(value::value_from_stack(lua, base + 1 + i)?);
    }
    sys::lua_settop(l, base);
    R::from_lua_multi(MultiValue::from_vec(out), lua)
}

#[cfg(test)]
mod tests;
