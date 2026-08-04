//! Raw FFI for the vendored Mercenaries-2 flavour of Lua 5.1.5.
//!
//! Every declaration here is **transcribed** from `vendor/lua.h`, `vendor/lauxlib.h`,
//! `vendor/lualib.h` and `vendor/luaconf.h` — not recalled. When the vendored tree moves, so does
//! this file, and `tests/vendor_integrity.rs` is what notices.
//!
//! # The one declaration that matters
//!
//! ```ignore
//! pub type lua_Number = f32;
//! ```
//!
//! This VM is built with `LUA_NUMBER float` (`luaconf.h`, patch 01), because that is what the game
//! shipped. It is also why this crate exists instead of a dependency: `mlua-sys` hardcodes
//! `pub type lua_Number = c_double`, so binding mlua to this library would push 8 bytes everywhere
//! the VM reads 4 — silent corruption, not a link error.
//!
//! Lua 5.1 has **no integer subtype**. `lua_Integer` is a C-side convenience that converts through
//! `lua_Number` on the way in and out, so it cannot carry more precision than an `f32` — 2^24. Use
//! [`lua_pushlightuserdata`] for handles, never [`lua_pushinteger`].
//!
//! # Safety
//!
//! Everything is `unsafe` and none of it is a safe abstraction. Two hazards the layer above must
//! own, neither of which this module can enforce:
//!
//! * **`longjmp`.** Lua 5.1 reports errors by unwinding with `longjmp` (`LUAI_THROW`). It does not
//!   run Rust destructors. Any Rust frame holding a value with a `Drop` impl must not be live
//!   across a call that can raise — which is every `luaL_*check*`, [`lua_error`], [`lua_call`],
//!   [`lua_gettable`] on a table with a metamethod, and allocation under memory pressure. Route
//!   fallible calls through [`lua_pcall`] / [`lua_cpcall`].
//! * **Panics.** A Rust `panic!` unwinding into C is undefined behaviour. Every `lua_CFunction`
//!   written in Rust must catch it at the boundary and convert to [`lua_error`] *after* its own
//!   frames have dropped.
//!
//! # Why every declaration here is `extern "C-unwind"`, not `extern "C"`
//!
//! Not a stylistic choice — plain `extern "C"` **aborts the process** on Windows, and the failing
//! mode is worth writing down because it costs an afternoon to rediscover.
//!
//! Lua raises by `longjmp`. On MSVC, `longjmp` is implemented as an SEH *forced unwind*, so it does
//! not merely reset the stack pointer — it walks frames. Rust marks `extern "C"` frames
//! `nounwind`, which plants an abort shim on that walk. A binding that returned `Err` therefore
//! died with `panic in a function that cannot unwind` / `STATUS_STACK_BUFFER_OVERRUN` rather than
//! surfacing an error to the script. `extern "C-unwind"` (RFC 2945) is the ABI that permits a
//! foreign unwind — including a forced one — to pass through.
//!
//! The obligation it carries is the one already stated above: a frame a `longjmp` crosses must have
//! **no live Rust destructor**. Permitting the unwind is not the same as making it safe to have
//! cleanup pending, so the raising paths in `rt` drop their values before jumping.

// Lua's own spellings are kept verbatim so this file can be diffed against the headers it
// transcribes: `lua_State` (not `LuaState`) and the `L` state parameter every API function takes.
#![allow(non_camel_case_types, non_snake_case)]

use std::os::raw::{c_char, c_int, c_void};

// ─── types ───────────────────────────────────────────────────────────────────────────────────

/// Opaque VM state. Never construct one; get it from [`luaL_newstate`].
#[repr(C)]
pub struct lua_State {
    _private: [u8; 0],
}

/// `LUA_NUMBER float` — the whole reason this crate vendors its own Lua. See the module docs.
pub type lua_Number = f32;

/// `LUA_INTEGER ptrdiff_t` (`luaconf.h:143`). Converts via `lua_Number`, so it is **not** a 64-bit
/// integer channel — anything above 2^24 loses precision. Handles go through lightuserdata.
pub type lua_Integer = isize;

pub type lua_CFunction = unsafe extern "C-unwind" fn(L: *mut lua_State) -> c_int;
pub type lua_Reader =
    unsafe extern "C-unwind" fn(L: *mut lua_State, ud: *mut c_void, sz: *mut usize) -> *const c_char;
pub type lua_Writer = unsafe extern "C-unwind" fn(
    L: *mut lua_State,
    p: *const c_void,
    sz: usize,
    ud: *mut c_void,
) -> c_int;
pub type lua_Alloc = unsafe extern "C-unwind" fn(
    ud: *mut c_void,
    ptr: *mut c_void,
    osize: usize,
    nsize: usize,
) -> *mut c_void;

/// `luaL_Reg` — one name/function row of a library table. This is the shape of the engine's own
/// `luaL_Reg` binding tables in the retail exe.
#[repr(C)]
pub struct luaL_Reg {
    pub name: *const c_char,
    pub func: Option<lua_CFunction>,
}

/// `lua_Debug` (`lua.h`). `short_src` is `LUA_IDSIZE` = 60 (`luaconf.h:210`).
#[repr(C)]
pub struct lua_Debug {
    pub event: c_int,
    pub name: *const c_char,
    pub namewhat: *const c_char,
    pub what: *const c_char,
    pub source: *const c_char,
    pub currentline: c_int,
    pub nups: c_int,
    pub linedefined: c_int,
    pub lastlinedefined: c_int,
    pub short_src: [c_char; LUA_IDSIZE],
    /* private part */
    i_ci: c_int,
}

// ─── constants ───────────────────────────────────────────────────────────────────────────────

pub const LUA_IDSIZE: usize = 60;
pub const LUA_MULTRET: c_int = -1;
pub const LUA_MINSTACK: c_int = 20;

/// Pseudo-indices. 5.1 addresses globals through a pseudo-index; 5.2 replaced this with `_ENV`.
pub const LUA_REGISTRYINDEX: c_int = -10000;
pub const LUA_ENVIRONINDEX: c_int = -10001;
pub const LUA_GLOBALSINDEX: c_int = -10002;

/// Thread status / call result codes. `0` is success and has no name in `lua.h`.
pub const LUA_OK: c_int = 0;
pub const LUA_YIELD: c_int = 1;
pub const LUA_ERRRUN: c_int = 2;
pub const LUA_ERRSYNTAX: c_int = 3;
pub const LUA_ERRMEM: c_int = 4;
pub const LUA_ERRERR: c_int = 5;
pub const LUA_ERRFILE: c_int = LUA_ERRERR + 1;

pub const LUA_TNONE: c_int = -1;
pub const LUA_TNIL: c_int = 0;
pub const LUA_TBOOLEAN: c_int = 1;
pub const LUA_TLIGHTUSERDATA: c_int = 2;
pub const LUA_TNUMBER: c_int = 3;
pub const LUA_TSTRING: c_int = 4;
pub const LUA_TTABLE: c_int = 5;
pub const LUA_TFUNCTION: c_int = 6;
pub const LUA_TUSERDATA: c_int = 7;
pub const LUA_TTHREAD: c_int = 8;

/// `luaL_ref` sentinels (`lauxlib.h`).
pub const LUA_NOREF: c_int = -2;
pub const LUA_REFNIL: c_int = -1;

/// `lua_gc` operations.
pub const LUA_GCSTOP: c_int = 0;
pub const LUA_GCRESTART: c_int = 1;
pub const LUA_GCCOLLECT: c_int = 2;
pub const LUA_GCCOUNT: c_int = 3;
pub const LUA_GCCOUNTB: c_int = 4;
pub const LUA_GCSTEP: c_int = 5;
pub const LUA_GCSETPAUSE: c_int = 6;
pub const LUA_GCSETSTEPMUL: c_int = 7;

// ─── lua.h ───────────────────────────────────────────────────────────────────────────────────

extern "C-unwind" {
    // state
    pub fn lua_newstate(f: lua_Alloc, ud: *mut c_void) -> *mut lua_State;
    pub fn lua_close(L: *mut lua_State);
    pub fn lua_atpanic(L: *mut lua_State, panicf: lua_CFunction) -> Option<lua_CFunction>;

    // stack
    pub fn lua_gettop(L: *mut lua_State) -> c_int;
    pub fn lua_settop(L: *mut lua_State, idx: c_int);
    pub fn lua_pushvalue(L: *mut lua_State, idx: c_int);
    pub fn lua_remove(L: *mut lua_State, idx: c_int);
    pub fn lua_insert(L: *mut lua_State, idx: c_int);
    pub fn lua_replace(L: *mut lua_State, idx: c_int);
    pub fn lua_checkstack(L: *mut lua_State, sz: c_int) -> c_int;

    // interrogation
    pub fn lua_isnumber(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_isstring(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_iscfunction(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_isuserdata(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_type(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_typename(L: *mut lua_State, tp: c_int) -> *const c_char;
    pub fn lua_equal(L: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int;
    pub fn lua_rawequal(L: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int;
    pub fn lua_lessthan(L: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int;

    // stack → Rust
    pub fn lua_tonumber(L: *mut lua_State, idx: c_int) -> lua_Number;
    pub fn lua_tointeger(L: *mut lua_State, idx: c_int) -> lua_Integer;
    pub fn lua_toboolean(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_tolstring(L: *mut lua_State, idx: c_int, len: *mut usize) -> *const c_char;
    pub fn lua_objlen(L: *mut lua_State, idx: c_int) -> usize;
    pub fn lua_tocfunction(L: *mut lua_State, idx: c_int) -> Option<lua_CFunction>;
    pub fn lua_touserdata(L: *mut lua_State, idx: c_int) -> *mut c_void;
    pub fn lua_tothread(L: *mut lua_State, idx: c_int) -> *mut lua_State;
    pub fn lua_topointer(L: *mut lua_State, idx: c_int) -> *const c_void;

    // Rust → stack
    pub fn lua_pushnil(L: *mut lua_State);
    pub fn lua_pushnumber(L: *mut lua_State, n: lua_Number);
    pub fn lua_pushinteger(L: *mut lua_State, n: lua_Integer);
    pub fn lua_pushlstring(L: *mut lua_State, s: *const c_char, l: usize);
    pub fn lua_pushstring(L: *mut lua_State, s: *const c_char);
    pub fn lua_pushcclosure(L: *mut lua_State, f: lua_CFunction, n: c_int);
    pub fn lua_pushboolean(L: *mut lua_State, b: c_int);
    pub fn lua_pushlightuserdata(L: *mut lua_State, p: *mut c_void);
    pub fn lua_pushthread(L: *mut lua_State) -> c_int;

    // get
    pub fn lua_gettable(L: *mut lua_State, idx: c_int);
    pub fn lua_getfield(L: *mut lua_State, idx: c_int, k: *const c_char);
    pub fn lua_rawget(L: *mut lua_State, idx: c_int);
    pub fn lua_rawgeti(L: *mut lua_State, idx: c_int, n: c_int);
    pub fn lua_createtable(L: *mut lua_State, narr: c_int, nrec: c_int);
    pub fn lua_newuserdata(L: *mut lua_State, sz: usize) -> *mut c_void;
    pub fn lua_getmetatable(L: *mut lua_State, objindex: c_int) -> c_int;
    pub fn lua_getfenv(L: *mut lua_State, idx: c_int);

    // set
    pub fn lua_settable(L: *mut lua_State, idx: c_int);
    pub fn lua_setfield(L: *mut lua_State, idx: c_int, k: *const c_char);
    pub fn lua_rawset(L: *mut lua_State, idx: c_int);
    pub fn lua_rawseti(L: *mut lua_State, idx: c_int, n: c_int);
    pub fn lua_setmetatable(L: *mut lua_State, objindex: c_int) -> c_int;
    /// The 5.1 module-environment primitive. This is what the original engine's `_SYS._IMPORT`
    /// used, and what replaces 5.4's `_ENV`-as-upvalue shimming.
    pub fn lua_setfenv(L: *mut lua_State, idx: c_int) -> c_int;

    // call / load
    pub fn lua_call(L: *mut lua_State, nargs: c_int, nresults: c_int);
    pub fn lua_pcall(L: *mut lua_State, nargs: c_int, nresults: c_int, errfunc: c_int) -> c_int;
    pub fn lua_cpcall(L: *mut lua_State, func: lua_CFunction, ud: *mut c_void) -> c_int;
    pub fn lua_load(
        L: *mut lua_State,
        reader: lua_Reader,
        dt: *mut c_void,
        chunkname: *const c_char,
    ) -> c_int;
    pub fn lua_dump(L: *mut lua_State, writer: lua_Writer, data: *mut c_void) -> c_int;

    // misc
    pub fn lua_status(L: *mut lua_State) -> c_int;
    pub fn lua_gc(L: *mut lua_State, what: c_int, data: c_int) -> c_int;
    /// Raises the value on the stack top as an error. **Never returns** — it `longjmp`s.
    pub fn lua_error(L: *mut lua_State) -> c_int;
    pub fn lua_next(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_concat(L: *mut lua_State, n: c_int);

    // debug (for error reporting; the corpus itself uses no `debug.*`)
    pub fn lua_getstack(L: *mut lua_State, level: c_int, ar: *mut lua_Debug) -> c_int;
    pub fn lua_getinfo(L: *mut lua_State, what: *const c_char, ar: *mut lua_Debug) -> c_int;
}

// ─── lauxlib.h / lualib.h ────────────────────────────────────────────────────────────────────

extern "C-unwind" {
    pub fn luaL_newstate() -> *mut lua_State;
    pub fn luaL_openlibs(L: *mut lua_State);
    pub fn luaL_register(L: *mut lua_State, libname: *const c_char, l: *const luaL_Reg);

    pub fn luaL_loadbuffer(
        L: *mut lua_State,
        buff: *const c_char,
        sz: usize,
        name: *const c_char,
    ) -> c_int;
    pub fn luaL_loadstring(L: *mut lua_State, s: *const c_char) -> c_int;

    /// Store the stack-top value in table `t` and return a key for [`lua_rawgeti`]. This is how a
    /// Rust-side handle keeps a Lua value alive; pair every ref with [`luaL_unref`].
    pub fn luaL_ref(L: *mut lua_State, t: c_int) -> c_int;
    pub fn luaL_unref(L: *mut lua_State, t: c_int, r: c_int);

    pub fn luaL_getmetafield(L: *mut lua_State, obj: c_int, e: *const c_char) -> c_int;
    pub fn luaL_callmeta(L: *mut lua_State, obj: c_int, e: *const c_char) -> c_int;
    pub fn luaL_newmetatable(L: *mut lua_State, tname: *const c_char) -> c_int;
    pub fn luaL_checkudata(L: *mut lua_State, ud: c_int, tname: *const c_char) -> *mut c_void;

    /// Pushes a `chunkname:line:` position string. Raises nothing, so it is safe to call while
    /// building an error message.
    pub fn luaL_where(L: *mut lua_State, lvl: c_int);

    // The `luaL_check*` family raises on mismatch — i.e. `longjmp`s. See the module safety notes.
    pub fn luaL_checklstring(L: *mut lua_State, numArg: c_int, l: *mut usize) -> *const c_char;
    pub fn luaL_checknumber(L: *mut lua_State, numArg: c_int) -> lua_Number;
    pub fn luaL_checkinteger(L: *mut lua_State, numArg: c_int) -> lua_Integer;
    pub fn luaL_checktype(L: *mut lua_State, narg: c_int, t: c_int);
    pub fn luaL_checkany(L: *mut lua_State, narg: c_int);
    pub fn luaL_checkstack(L: *mut lua_State, sz: c_int, msg: *const c_char);
    pub fn luaL_argerror(L: *mut lua_State, numarg: c_int, extramsg: *const c_char) -> c_int;
}

// ─── macros ──────────────────────────────────────────────────────────────────────────────────
//
// These are `#define`s in `lua.h`, so they export no symbol and must be re-expressed. Each carries
// the line it was transcribed from.

/// `lua.h:39` — upvalue pseudo-index. The closure trampoline reads its boxed Rust callback from
/// `lua_upvalueindex(1)`.
#[inline]
pub const fn lua_upvalueindex(i: c_int) -> c_int {
    LUA_GLOBALSINDEX - i
}

/// `lua.h:254`
///
/// # Safety
/// `n` values must actually be on the stack.
#[inline]
pub unsafe fn lua_pop(L: *mut lua_State, n: c_int) {
    lua_settop(L, -n - 1)
}

/// `lua.h:256`
///
/// # Safety
/// `L` must be a live state with stack space (see [`lua_checkstack`]).
#[inline]
pub unsafe fn lua_newtable(L: *mut lua_State) {
    lua_createtable(L, 0, 0)
}

/// `lua.h:260`
///
/// # Safety
/// `L` must be a live state with stack space.
#[inline]
pub unsafe fn lua_pushcfunction(L: *mut lua_State, f: lua_CFunction) {
    lua_pushcclosure(L, f, 0)
}

/// `lua.h:262`
///
/// # Safety
/// `idx` must be a valid stack index.
#[inline]
pub unsafe fn lua_strlen(L: *mut lua_State, i: c_int) -> usize {
    lua_objlen(L, i)
}

/// `lua.h:276`
///
/// # Safety
/// `s` must be a NUL-terminated C string; a value must be on the stack top.
#[inline]
pub unsafe fn lua_setglobal(L: *mut lua_State, s: *const c_char) {
    lua_setfield(L, LUA_GLOBALSINDEX, s)
}

/// `lua.h:277`
///
/// # Safety
/// `s` must be a NUL-terminated C string.
#[inline]
pub unsafe fn lua_getglobal(L: *mut lua_State, s: *const c_char) {
    lua_getfield(L, LUA_GLOBALSINDEX, s)
}

/// `lua.h:279`
///
/// # Safety
/// `idx` must be a valid stack index. The returned pointer is owned by Lua and is invalidated by
/// the next GC that can collect the string — copy before yielding control back.
#[inline]
pub unsafe fn lua_tostring(L: *mut lua_State, i: c_int) -> *const c_char {
    lua_tolstring(L, i, std::ptr::null_mut())
}

/// `lua.h:264-271` — the type predicates, which are `lua_type` comparisons rather than calls.
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_isfunction(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) == LUA_TFUNCTION
}

/// See [`lua_isfunction`].
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_istable(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) == LUA_TTABLE
}

/// See [`lua_isfunction`].
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_islightuserdata(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) == LUA_TLIGHTUSERDATA
}

/// See [`lua_isfunction`].
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_isnil(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) == LUA_TNIL
}

/// See [`lua_isfunction`].
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_isboolean(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) == LUA_TBOOLEAN
}

/// See [`lua_isfunction`].
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_isnone(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) == LUA_TNONE
}

/// `lua.h:271` — none *or* nil, which is the "argument absent" test.
///
/// # Safety
/// `n` must be a valid stack index or pseudo-index.
#[inline]
pub unsafe fn lua_isnoneornil(L: *mut lua_State, n: c_int) -> bool {
    lua_type(L, n) <= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declaration the whole crate turns on. If this ever reads 8, the vendored `luaconf.h`
    /// lost patch 01 — or somebody "fixed" this file to match stock Lua.
    #[test]
    fn lua_number_is_single_precision() {
        assert_eq!(
            std::mem::size_of::<lua_Number>(),
            4,
            "lua_Number must be f32 — the game's VM was built with LUA_NUMBER float"
        );
    }

    /// A round trip through the real VM, proving the `extern` block links and that the float
    /// declaration matches the library's ABI. A `c_double` mismatch shows up here as garbage.
    #[test]
    fn pushes_and_reads_a_number_through_the_real_vm() {
        unsafe {
            let l = luaL_newstate();
            assert!(!l.is_null());
            lua_pushnumber(l, 0.5);
            assert_eq!(lua_tonumber(l, -1), 0.5);
            assert_eq!(lua_type(l, -1), LUA_TNUMBER);
            lua_pop(l, 1);
            assert_eq!(lua_gettop(l), 0, "stack must be balanced");
            lua_close(l);
        }
    }

    /// `lua_pushinteger` converts through `lua_Number`, so it is NOT a 64-bit channel. Pinning the
    /// loss here stops anyone routing a GUID through it — handles travel as lightuserdata.
    #[test]
    fn integers_lose_precision_above_2_pow_24() {
        unsafe {
            let l = luaL_newstate();
            let big: lua_Integer = (1 << 24) + 1;
            lua_pushinteger(l, big);
            let back = lua_tointeger(l, -1);
            assert_ne!(back, big, "f32 cannot hold 2^24+1 — use lightuserdata for handles");
            lua_pop(l, 1);
            lua_close(l);
        }
    }

    /// Lightuserdata is the handle channel, and it must survive the full pointer width.
    #[test]
    fn lightuserdata_round_trips_a_full_pointer() {
        unsafe {
            let l = luaL_newstate();
            let p = 0x1234_5678_9abc_def0u64 as usize as *mut c_void;
            lua_pushlightuserdata(l, p);
            assert!(lua_islightuserdata(l, -1));
            assert_eq!(lua_touserdata(l, -1), p);
            lua_pop(l, 1);
            lua_close(l);
        }
    }

    /// The standard library must open — the engine's Lua bindings assume `table`, `string`, `math`, and
    /// the corpus additionally uses `os.*` and `io.*`.
    #[test]
    fn openlibs_provides_the_stdlib_the_corpus_uses() {
        unsafe {
            let l = luaL_newstate();
            luaL_openlibs(l);
            for lib in [c"table", c"string", c"math", c"os", c"io"] {
                lua_getglobal(l, lib.as_ptr());
                assert!(lua_istable(l, -1), "{lib:?} must be a table after luaL_openlibs");
                lua_pop(l, 1);
            }
            // 5.1 natives the corpus leans on: `unpack` (76 uses) and `getfenv` (18).
            for f in [c"unpack", c"getfenv", c"setfenv", c"loadstring"] {
                lua_getglobal(l, f.as_ptr());
                assert!(lua_isfunction(l, -1), "{f:?} must be a native global in 5.1");
                lua_pop(l, 1);
            }
            lua_getglobal(l, c"table".as_ptr());
            lua_getfield(l, -1, c"getn".as_ptr());
            assert!(lua_isfunction(l, -1), "table.getn must exist (112 corpus uses)");
            lua_pop(l, 2);
            assert_eq!(lua_gettop(l), 0);
            lua_close(l);
        }
    }

    /// The implicit vararg `arg` table (`LUA_COMPAT_VARARG`). Two corpus files depend on it —
    /// `resident/mrxtaskjobdestroyset.lua` and `mrxtaskjobverifyset.lua` — and it is a *hidden
    /// local* created by the VM, so no Lua-level prelude can substitute for it. This executes the
    /// real thing rather than trusting the `#define`.
    #[test]
    fn implicit_vararg_arg_table_exists() {
        unsafe {
            let l = luaL_newstate();
            luaL_openlibs(l);
            let src = c"function f(self, ...) return arg[1], arg.n end return f(0, 11, 22)";
            assert_eq!(luaL_loadstring(l, src.as_ptr()), LUA_OK, "chunk must compile");
            assert_eq!(lua_pcall(l, 0, 2, 0), LUA_OK, "chunk must run");
            assert_eq!(lua_tonumber(l, -2), 11.0, "arg[1] must be the first extra argument");
            assert_eq!(lua_tonumber(l, -1), 2.0, "arg.n must count the extra arguments");
            lua_pop(l, 2);
            lua_close(l);
        }
    }
}
