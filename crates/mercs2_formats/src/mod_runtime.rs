//! Bindings to m2-sdk.dll, the runtime every Mercenaries 2 mod shares in the game process (built by
//! `crates/mercs2_mod_runtime`, placed in the game root by the m2-sdk Shipment). A Rust mod calls the one
//! loaded DLL through these: nothing of the runtime is compiled into the mod, so hooks, captured Lua
//! states, namespaces and callbacks stay single for the whole process.
//!
//! The imports are `raw-dylib`: rustc writes the import entries for the functions a binary calls, so no
//! import library is needed and a binary that calls none imports nothing. Declared for the 32-bit Windows
//! target only. The C contract is the Shipment's `m2/*.h`; the semantics are documented there and on the
//! safe wrappers below.

use core::ffi::{c_char, c_int, c_void};

pub type Hmodule = *mut c_void;
/// `int f(void* L)`, cdecl: a Lua C function, returning how many values it pushed.
pub type LuaCFunction = unsafe extern "C" fn(l: *mut c_void) -> c_int;
pub type LuaStateCb = unsafe extern "C" fn(l: *mut c_void, is_new_cycle: c_int, ud: *mut c_void);
pub type LuaTickCb = unsafe extern "C" fn(l: *mut c_void, ud: *mut c_void);
pub type LogListener = unsafe extern "C" fn(msg: *const c_char, ud: *mut c_void);
pub type PhaseCb = unsafe extern "C" fn(reached_idx: c_int, ud: *mut c_void);
pub type IniKv = unsafe extern "C" fn(ud: *mut c_void, key: *const c_char, value: *const c_char);

/// `m2_lua_reg`: one namespace row; a table ends with a NULL name.
#[repr(C)]
pub struct LuaReg {
    pub name: *const c_char,
    pub func: Option<LuaCFunction>,
}

// SAFETY: a table of 'static names and functions, never written.
unsafe impl Sync for LuaReg {}

/// `m2_shader_class`.
#[repr(C)]
pub struct ShaderClass {
    pub name: *const c_char,
    pub sho: *const c_char,
}

/// `m2_lua_exec`'s result codes.
pub const EXEC_OK: c_int = 0;
pub const EXEC_COMPILE: c_int = 1;
pub const EXEC_RUNTIME: c_int = 2;
pub const EXEC_REFUSED: c_int = 3;

#[cfg(all(windows, target_arch = "x86"))]
pub mod ffi {
    use super::*;

    #[link(name = "m2-sdk", kind = "raw-dylib", import_name_type = "undecorated")]
    unsafe extern "C" {
        pub fn m2_version_num() -> c_int;
        pub fn m2_version_string() -> *const c_char;
        pub fn m2_hook_init() -> c_int;
        pub fn m2_hook_attach(target: *mut c_void, detour: *mut c_void, orig: *mut *mut c_void) -> c_int;
        pub fn m2_ini_parse(path: *const c_char, cb: Option<IniKv>, ud: *mut c_void) -> c_int;
        pub fn m2_ini_bool(value: *const c_char) -> c_int;
        pub fn m2_ini_int(value: *const c_char, fallback: c_int) -> c_int;
        pub fn m2_log_init(module: Hmodule);
        pub fn m2_logf_mod(module: Hmodule, fmt: *const c_char, ...);
        pub fn m2_log_close_mod(module: Hmodule);
        pub fn m2_module_path(module: Hmodule, filename: *const c_char, out: *mut c_char, out_size: c_int);
        pub fn m2_lua_nargs(l: *mut c_void) -> c_int;
        pub fn m2_lua_arg_string(l: *mut c_void, idx0: c_int, out: *mut c_char, out_max: c_int) -> c_int;
        pub fn m2_lua_arg_type(l: *mut c_void, idx0: c_int) -> c_int;
        pub fn m2_lua_arg_number(l: *mut c_void, idx0: c_int, out: *mut f64) -> c_int;
        pub fn m2_lua_arg_bool(l: *mut c_void, idx0: c_int, out: *mut c_int) -> c_int;
        pub fn m2_lua_join_strings(l: *mut c_void, out: *mut c_char, out_max: c_int) -> c_int;
        pub fn m2_loghook_add_listener(cb: Option<LogListener>, ud: *mut c_void) -> c_int;
        pub fn m2_loghook_install() -> c_int;
        pub fn m2_loghook_set_active(cb: Option<LogListener>, ud: *mut c_void, active: c_int) -> c_int;
        pub fn m2_loadtrigger_on_phase(target_idx: c_int, cb: Option<PhaseCb>, ud: *mut c_void) -> c_int;
        pub fn m2_loadtrigger_install() -> c_int;
        pub fn m2_loadtrigger_current_phase() -> c_int;
        pub fn m2_loadtrigger_phase_name(idx: c_int) -> *const c_char;
        pub fn m2_shader_add_pixel(family: c_int, classes: *const ShaderClass) -> c_int;
        pub fn m2_shader_add_vertex(family: c_int, cls: *const ShaderClass) -> c_int;
        pub fn m2_shader_outcome(name: *const c_char) -> c_int;
        pub fn m2_shader_status_name(s: c_int) -> *const c_char;
        pub fn m2_lua_install() -> c_int;
        pub fn m2_lua_last_error() -> *const c_char;
        pub fn m2_lua_add_namespace(name: *const c_char, fns: *const LuaReg) -> c_int;
        pub fn m2_lua_on_state(cb: Option<LuaStateCb>, ud: *mut c_void) -> c_int;
        pub fn m2_lua_on_tick(cb: Option<LuaTickCb>, ud: *mut c_void) -> c_int;
        pub fn m2_lua_exec(l: *mut c_void, chunk: *const c_char, len: c_int, out: *mut c_char, cap: c_int) -> c_int;
        pub fn m2_lua_state_ok(l: *mut c_void) -> c_int;
        pub fn m2_lua_in_exec() -> c_int;
        pub fn m2_lua_raise(l: *mut c_void, msg: *const c_char) -> !;
        pub fn m2_lua_push_nil(l: *mut c_void);
        pub fn m2_lua_push_bool(l: *mut c_void, b: c_int);
        pub fn m2_lua_push_number(l: *mut c_void, n: f64);
        pub fn m2_lua_push_string(l: *mut c_void, s: *const c_char, len: c_int);
    }
}

/// `M2_VERSION_NUM` of a version: `major * 10000 + minor * 100 + patch` (`m2_version.h`).
pub fn version_num(major: i32, minor: i32, patch: i32) -> i32 {
    major * 10000 + minor * 100 + patch
}

/// Does the loaded DLL's `m2_version_num()` satisfy `^built` (`X.Y.Z`)? The caret rule of a Shipment's
/// `load.requires`: the same major, and for major 0 the same minor; and no older than `built`.
pub fn compatible(loaded: i32, built: &str) -> Result<(), String> {
    let parts: Vec<&str> = built.split('.').collect();
    let [ma, mi, pa] = parts[..] else { return Err(format!("m2-sdk version {built:?} is not X.Y.Z")) };
    let n = |s: &str| s.parse::<i32>().ok().filter(|v| (0..100).contains(v)).ok_or_else(|| format!("m2-sdk version {built:?} has a part outside 0-99"));
    let want = version_num(n(ma)?, n(mi)?, n(pa)?);
    let (lmaj, lmin) = (loaded / 10000, loaded / 100 % 100);
    let (wmaj, wmin) = (want / 10000, want / 100 % 100);
    let show = |v: i32| format!("{}.{}.{}", v / 10000, v / 100 % 100, v % 100);
    if lmaj != wmaj || (wmaj == 0 && lmin != wmin) || loaded < want {
        return Err(format!(
            "m2-sdk.dll in the game folder is version {}, but this mod needs ^{} (built against {}); install the m2-sdk \
             Shipment version it requires",
            show(loaded),
            if wmaj == 0 { format!("0.{wmin}") } else { wmaj.to_string() },
            show(want)
        ));
    }
    Ok(())
}

#[cfg(all(windows, target_arch = "x86"))]
pub use safe::*;

#[cfg(all(windows, target_arch = "x86"))]
mod safe {
    use super::*;
    use core::ffi::CStr;
    use std::sync::atomic::AtomicUsize;

    /// The loaded DLL's version number.
    pub fn loaded_version() -> i32 {
        unsafe { ffi::m2_version_num() }
    }

    /// Create and enable a detour through the process's one MinHook. `orig` receives the trampoline before
    /// the detour can first run.
    ///
    /// # Safety
    /// `target` starts a function; `detour` has its signature.
    pub unsafe fn hook_attach(target: *mut c_void, detour: *mut c_void, orig: Option<&AtomicUsize>, what: &str) -> Result<(), String> {
        let slot = orig.map_or(core::ptr::null_mut(), |o| o.as_ptr().cast());
        if unsafe { ffi::m2_hook_attach(target, detour, slot) } != 1 {
            return Err(format!("m2_hook_attach failed for {what} at {target:p}"));
        }
        Ok(())
    }

    fn last_error() -> String {
        let p = unsafe { ffi::m2_lua_last_error() };
        if p.is_null() { "no message".into() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
    }

    /// Install the Lua host. Waits up to 30 s for the game's code; never call it under the loader lock.
    pub fn lua_install() -> Result<(), String> {
        if unsafe { ffi::m2_lua_install() } == 1 { Ok(()) } else { Err(last_error()) }
    }

    /// Add the global table `name` of `fns` (ending with a NULL-name row) to every state.
    pub fn lua_add_namespace(name: &'static CStr, fns: &'static [LuaReg]) -> Result<(), String> {
        match fns.last() {
            Some(last) if last.name.is_null() => {}
            _ => return Err(format!("namespace {}: the table must end with a NULL-name row", name.to_string_lossy())),
        }
        if unsafe { ffi::m2_lua_add_namespace(name.as_ptr(), fns.as_ptr()) } == 1 { Ok(()) } else { Err(last_error()) }
    }

    /// # Safety
    /// `cb` may be called with `ud` on the game thread for the life of the process.
    pub unsafe fn lua_on_state(cb: LuaStateCb, ud: *mut c_void) -> Result<(), String> {
        if unsafe { ffi::m2_lua_on_state(Some(cb), ud) } == 1 { Ok(()) } else { Err("m2_lua_on_state: all 16 slots are taken".into()) }
    }

    /// # Safety
    /// `cb` may be called with `ud` on the game thread for the life of the process.
    pub unsafe fn lua_on_tick(cb: LuaTickCb, ud: *mut c_void) -> Result<(), String> {
        if unsafe { ffi::m2_lua_on_tick(Some(cb), ud) } == 1 { Ok(()) } else { Err("m2_lua_on_tick: all 16 slots are taken".into()) }
    }

    /// Add a log listener and start the log source (`m2/m2_loghook.h`).
    ///
    /// # Safety
    /// `cb` may be called with `ud` on any thread that logs, for the life of the process.
    pub unsafe fn loghook_listen(cb: LogListener, ud: *mut c_void) -> Result<(), String> {
        if unsafe { ffi::m2_loghook_add_listener(Some(cb), ud) } != 1 {
            return Err("m2_loghook_add_listener: all 16 slots are taken".into());
        }
        if unsafe { ffi::m2_loghook_install() } != 1 {
            return Err("m2_loghook_install failed; m2-sdk.log has the reason".into());
        }
        Ok(())
    }

    /// Pause or resume a listener added with [`loghook_listen`].
    pub fn loghook_set_active(cb: LogListener, ud: *mut c_void, active: bool) -> Result<(), String> {
        if unsafe { ffi::m2_loghook_set_active(Some(cb), ud, c_int::from(active)) } != 1 {
            return Err("m2_loghook_set_active: no such listener".into());
        }
        Ok(())
    }

    /// A chunk's outcome and its result line.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ExecResult {
        pub status: c_int,
        pub text: Vec<u8>,
    }

    /// Run `chunk` on `l`; `cap` bounds the result line.
    ///
    /// # Safety
    /// On the game thread, with the `L` of a state/tick callback or of a running Lua C function.
    pub unsafe fn lua_exec(l: *mut c_void, chunk: &[u8], cap: usize) -> ExecResult {
        let mut out = vec![0u8; cap.max(1)];
        let len = c_int::try_from(chunk.len()).unwrap_or(c_int::MAX);
        let status =
            unsafe { ffi::m2_lua_exec(l, chunk.as_ptr().cast(), len, out.as_mut_ptr().cast(), out.len() as c_int) };
        let n = out.iter().position(|&b| b == 0).unwrap_or(out.len());
        out.truncate(n);
        ExecResult { status, text: out }
    }

    /// Whether `l` is a usable `lua_State` right now.
    pub fn lua_state_ok(l: *mut c_void) -> bool {
        unsafe { ffi::m2_lua_state_ok(l) == 1 }
    }

    pub fn lua_in_exec() -> bool {
        let depth = unsafe { ffi::m2_lua_in_exec() };
        depth != 0
    }
}

/// The Lua stack as a mod's bindings see it: arguments read and results pushed through the DLL's
/// `m2_lua_*` exports in the game ([`lua::game`]), and through [`lua::fake::FakeLua`] in host tests.
/// Argument indices are 0-based, as `m2_lua_arg_*` take them; an absent argument has type -1.
pub mod lua {
    /// Lua 5.1 type tags (`m2/m2_luastack.h`).
    pub const TNIL: i32 = 0;
    pub const TBOOLEAN: i32 = 1;
    pub const TNUMBER: i32 = 3;
    pub const TSTRING: i32 = 4;
    pub const TTABLE: i32 = 5;
    pub const TFUNCTION: i32 = 6;

    /// The longest string argument a binding reads, in bytes.
    pub const MAX_ARG_STRING: usize = 64 * 1024;

    pub trait Lua {
        /// The argument's type tag, or -1 when it is absent.
        fn arg_type(&self, i: i32) -> i32;
        /// How many arguments were passed.
        fn arg_count(&self) -> i32 {
            let mut n = 0;
            while self.arg_type(n) != -1 {
                n += 1;
            }
            n
        }
        fn arg_number(&self, i: i32) -> Option<f64>;
        /// Lua truthiness of a present argument (only nil and false are false).
        fn arg_bool(&self, i: i32) -> Option<bool>;
        /// A string argument's bytes, up to its first NUL and [`MAX_ARG_STRING`].
        fn arg_string(&self, i: i32) -> Option<Vec<u8>>;
        fn push_nil(&mut self);
        fn push_bool(&mut self, b: bool);
        fn push_number(&mut self, n: f64);
        fn push_string(&mut self, s: &[u8]);
    }

    /// The type name `type()` reports for a tag.
    pub fn type_name(tag: i32) -> &'static str {
        match tag {
            -1 => "no value",
            TNIL => "nil",
            TBOOLEAN => "boolean",
            2 | 7 => "userdata",
            TNUMBER => "number",
            TSTRING => "string",
            TTABLE => "table",
            TFUNCTION => "function",
            8 => "thread",
            _ => "unknown",
        }
    }

    /// What a binding wants returned to Lua.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Ret {
        /// This many values were pushed.
        Values(i32),
        /// Raise a Lua error with this message (nothing was pushed).
        Raise(String),
    }

    #[cfg(all(windows, target_arch = "x86"))]
    pub mod game {
        //! The game's stack through m2-sdk.dll.
        use super::*;
        use core::ffi::{c_char, c_void};
        use crate::mod_runtime::ffi;

        pub struct GameLua(pub *mut c_void);

        impl Lua for GameLua {
            fn arg_type(&self, i: i32) -> i32 {
                unsafe { ffi::m2_lua_arg_type(self.0, i) }
            }
            fn arg_number(&self, i: i32) -> Option<f64> {
                let mut v = 0.0;
                (unsafe { ffi::m2_lua_arg_number(self.0, i, &mut v) } == 1).then_some(v)
            }
            fn arg_bool(&self, i: i32) -> Option<bool> {
                let mut v = 0;
                (unsafe { ffi::m2_lua_arg_bool(self.0, i, &mut v) } == 1).then_some(v != 0)
            }
            fn arg_string(&self, i: i32) -> Option<Vec<u8>> {
                if self.arg_type(i) != TSTRING {
                    return None;
                }
                let mut buf = vec![0u8; MAX_ARG_STRING + 1];
                let n = unsafe { ffi::m2_lua_arg_string(self.0, i, buf.as_mut_ptr().cast::<c_char>(), buf.len() as i32) };
                buf.truncate(n.max(0) as usize);
                Some(buf)
            }
            fn push_nil(&mut self) {
                unsafe { ffi::m2_lua_push_nil(self.0) }
            }
            fn push_bool(&mut self, b: bool) {
                unsafe { ffi::m2_lua_push_bool(self.0, i32::from(b)) }
            }
            fn push_number(&mut self, n: f64) {
                unsafe { ffi::m2_lua_push_number(self.0, n) }
            }
            fn push_string(&mut self, s: &[u8]) {
                unsafe { ffi::m2_lua_push_string(self.0, s.as_ptr().cast::<c_char>(), s.len() as i32) }
            }
        }

        /// Run a binding on the game's stack: its pushed count, or a raised error. `m2_lua_raise` does not
        /// return, so the message is leaked and the binding has dropped everything it owned by then.
        ///
        /// # Safety
        /// `l` is the `lua_State` the game passed to the Lua C function this runs inside, on the game thread.
        pub unsafe fn call(l: *mut c_void, f: impl FnOnce(&mut GameLua) -> Ret) -> i32 {
            let ret = f(&mut GameLua(l));
            match ret {
                Ret::Values(n) => n,
                Ret::Raise(msg) => {
                    let c = std::ffi::CString::new(msg.replace('\0', " ")).expect("NULs replaced");
                    let p = c.into_raw();
                    unsafe { ffi::m2_lua_raise(l, p) }
                }
            }
        }
    }

    pub mod fake {
        //! A stack for a mod's host tests: the arguments given, and the values pushed.
        use super::*;

        #[derive(Debug, Clone, PartialEq)]
        pub enum Val {
            Nil,
            Bool(bool),
            Num(f64),
            Str(Vec<u8>),
            Table,
        }

        #[derive(Default)]
        pub struct FakeLua {
            pub args: Vec<Val>,
            pub pushed: Vec<Val>,
        }

        impl FakeLua {
            pub fn with(args: Vec<Val>) -> Self {
                FakeLua { args, pushed: Vec::new() }
            }
            fn arg(&self, i: i32) -> Option<&Val> {
                usize::try_from(i).ok().and_then(|i| self.args.get(i))
            }
        }

        pub fn s(text: &str) -> Val {
            Val::Str(text.as_bytes().to_vec())
        }

        impl Lua for FakeLua {
            fn arg_type(&self, i: i32) -> i32 {
                match self.arg(i) {
                    None => -1,
                    Some(Val::Nil) => TNIL,
                    Some(Val::Bool(_)) => TBOOLEAN,
                    Some(Val::Num(_)) => TNUMBER,
                    Some(Val::Str(_)) => TSTRING,
                    Some(Val::Table) => TTABLE,
                }
            }
            /// The game's `lua_Number` is a 32-bit float: values round through `f32` as they would there.
            fn arg_number(&self, i: i32) -> Option<f64> {
                match self.arg(i) {
                    Some(Val::Num(n)) => Some(f64::from(*n as f32)),
                    _ => None,
                }
            }
            fn arg_bool(&self, i: i32) -> Option<bool> {
                self.arg(i).map(|v| !matches!(v, Val::Nil | Val::Bool(false)))
            }
            fn arg_string(&self, i: i32) -> Option<Vec<u8>> {
                match self.arg(i) {
                    Some(Val::Str(b)) => {
                        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len()).min(MAX_ARG_STRING);
                        Some(b[..end].to_vec())
                    }
                    _ => None,
                }
            }
            fn push_nil(&mut self) {
                self.pushed.push(Val::Nil);
            }
            fn push_bool(&mut self, b: bool) {
                self.pushed.push(Val::Bool(b));
            }
            fn push_number(&mut self, n: f64) {
                self.pushed.push(Val::Num(f64::from(n as f32)));
            }
            fn push_string(&mut self, s: &[u8]) {
                self.pushed.push(Val::Str(s.to_vec()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_stack_reads_arguments_and_records_pushes() {
        use super::lua::fake::{FakeLua, Val, s};
        use super::lua::{Lua, TNIL, TSTRING, type_name};
        let mut l = FakeLua::with(vec![s("a\0b"), Val::Nil, Val::Num(0.1)]);
        assert_eq!(l.arg_count(), 3);
        assert_eq!((l.arg_type(0), l.arg_type(1), l.arg_type(3)), (TSTRING, TNIL, -1));
        assert_eq!(l.arg_string(0), Some(b"a".to_vec()), "a string argument stops at its first NUL");
        assert_eq!(l.arg_number(2), Some(f64::from(0.1f32)), "numbers are the game's 32-bit floats");
        assert_eq!(l.arg_bool(1), Some(false));
        assert_eq!(l.arg_bool(3), None);
        assert_eq!(FakeLua::default().arg_count(), 0);
        l.push_string(b"x");
        l.push_bool(true);
        assert_eq!(l.pushed, vec![s("x"), Val::Bool(true)]);
        assert_eq!((type_name(-1), type_name(TSTRING)), ("no value", "string"));
    }

    #[test]
    fn the_caret_rule_matches_load_requires() {
        assert_eq!(compatible(400, "0.4.0"), Ok(()));
        assert_eq!(compatible(409, "0.4.1"), Ok(()));
        assert!(compatible(401, "0.4.2").unwrap_err().contains("version 0.4.1"));
        assert!(compatible(500, "0.4.0").is_err(), "0.5 is a breaking line under 0.x");
        assert!(compatible(10000, "0.4.0").is_err());
        assert_eq!(compatible(10203, "1.2.0"), Ok(()));
        assert!(compatible(20000, "1.2.0").is_err());
        assert!(compatible(400, "0.4").is_err());
    }
}
