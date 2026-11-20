//! The Lua host (`m2/m2_lua.h`): one set of detours in the process for every mod that adds Lua functions
//! or runs Lua.
//!
//! * The script host's namespace walker ([`Site::RegisterNamespaces`]) registers every added namespace on
//!   each `lua_State` the host creates, right after the engine's own and before any script of that state
//!   runs.
//! * `luaB_type` captures the states scripts run on: a state not seen recently is validated, gets every
//!   namespace, and is announced to the state callbacks, flagged new when this process has never seen it
//!   (a fresh state after a completed load is the next load cycle: a main-menu return builds fresh states).
//! * `luaB_type`, `CreateTextWidget` and the shared log stub are the game-thread ticks: each call on a
//!   captured state runs the tick callbacks, except while a chunk runs on that thread.
//! * [`m2_lua_exec`] runs a chunk with the game's `luaB_loadstring` and `luaB_pcall` on top of the frame of
//!   the function it is called from, and restores that frame.
//! * [`m2_lua_ref`], [`m2_lua_unref`], [`m2_lua_push_ref`] and [`m2_lua_pcall`] keep a Lua value in the
//!   registry and call it later, through the game's `luaL_ref`, `luaL_unref`, `lua_rawgeti` and `lua_pcall`
//!   (`mercs2_formats::lua_sites::RefSites`); [`m2_lua_pop`] removes values a caller pushed.
//!
//! Every detoured function is a cdecl `lua_CFunction` except the walker (`__stdcall (host)`).

use crate::{hook, luastack, rt_log, sites, stub};
use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use mercs2_formats::lua_exec::{self, MAX_CHUNK, MAX_RESULTS};
use mercs2_formats::lua_sites::{HOST_INITIALISED, HOST_LUA_STATE, LuaSites, Site};
use mercs2_formats::mod_runtime::lua::type_name;
use mercs2_formats::mod_runtime::{LastError, PCALL_REFUSED, RefTable, namespace_add_refused};
use mercs2_formats::lua_stack::{
    self, LUA_REGISTRYINDEX, LUA_TBOOLEAN, LUA_TFUNCTION, LUA_TNIL, LUA_TNUMBER, LUA_TSTRING, LUA_TTABLE, LUA_TTHREAD, Mem,
    OFF_BASE, OFF_BASE_CI, OFF_CI, OFF_GLOBAL, OFF_STACK, OFF_TOP, TVALUE_SIZE, TVALUE_TT,
};
use std::ffi::CString;
use std::sync::{Mutex, MutexGuard};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

pub type LuaCFunction = unsafe extern "C" fn(l: *mut c_void) -> c_int;
pub type StateCb = unsafe extern "C" fn(l: *mut c_void, is_new_cycle: c_int, ud: *mut c_void);
pub type TickCb = unsafe extern "C" fn(l: *mut c_void, ud: *mut c_void);

/// `luaL_Reg` / `m2_lua_reg`: `{ const char* name; lua_CFunction func; }`, ended by a NULL name.
#[repr(C)]
pub struct LuaReg {
    pub name: *const c_char,
    pub func: Option<LuaCFunction>,
}

const MAX_NAMESPACES: usize = 32;
const MAX_FUNCTIONS: usize = 256;
const MAX_CALLBACKS: usize = 16;
const SEEN_SLOTS: usize = 16;
const EVER_SLOTS: usize = 32;
/// A frame larger than this is saved only up to it (the measured distance is 4 slots).
const FRAME_SAVE_MAX: u32 = 16 * TVALUE_SIZE;

pub const EXEC_OK: c_int = 0;
pub const EXEC_COMPILE: c_int = 1;
pub const EXEC_RUNTIME: c_int = 2;
pub const EXEC_REFUSED: c_int = 3;

struct Namespace {
    name: CString,
    names: Vec<CString>,
    regs: Vec<LuaReg>,
}
// SAFETY: the rows point into `names`, owned by the same value, and at functions; read only under the lock.
unsafe impl Send for Namespace {}

struct Slot {
    cb: AtomicPtr<c_void>,
    ud: AtomicPtr<c_void>,
}

static NAMESPACES: Mutex<Vec<Namespace>> = Mutex::new(Vec::new());
static NS_COUNT: AtomicUsize = AtomicUsize::new(0);
/// A namespace was added after some captured state got its namespaces.
static REG_PENDING: AtomicBool = AtomicBool::new(false);

static STATE_CBS: [Slot; MAX_CALLBACKS] =
    [const { Slot { cb: AtomicPtr::new(core::ptr::null_mut()), ud: AtomicPtr::new(core::ptr::null_mut()) } }; MAX_CALLBACKS];
static STATE_N: AtomicI32 = AtomicI32::new(0);
static TICK_CBS: [Slot; MAX_CALLBACKS] =
    [const { Slot { cb: AtomicPtr::new(core::ptr::null_mut()), ud: AtomicPtr::new(core::ptr::null_mut()) } }; MAX_CALLBACKS];
static TICK_N: AtomicI32 = AtomicI32::new(0);

static CURRENT: AtomicU32 = AtomicU32::new(0);
static SEEN: [AtomicU32; SEEN_SLOTS] = [const { AtomicU32::new(0) }; SEEN_SLOTS];
/// How many namespaces are registered on each [`SEEN`] state.
static SEEN_REG: [AtomicUsize; SEEN_SLOTS] = [const { AtomicUsize::new(0) }; SEEN_SLOTS];
static SEEN_NEXT: AtomicUsize = AtomicUsize::new(0);
/// Every state ever captured; never cleared, so only a genuinely new state is a new load cycle.
static EVER: [AtomicU32; EVER_SLOTS] = [const { AtomicU32::new(0) }; EVER_SLOTS];
static EVER_N: AtomicUsize = AtomicUsize::new(0);

static INSTALLED: Mutex<bool> = Mutex::new(false);
/// [`m2_lua_last_error`]'s message, one per thread.
static LAST_ERROR: LastError = LastError::new(&LAST_ERROR_SLOT);
static ORIG_WALKER: AtomicUsize = AtomicUsize::new(0);
static ORIG_TYPE: AtomicUsize = AtomicUsize::new(0);
static ORIG_WIDGET: AtomicUsize = AtomicUsize::new(0);
static ORIG_FRAME: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static EXEC_DEPTH: Cell<u32> = const { Cell::new(0) };
    static LAST_ERROR_SLOT: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn fail(msg: String) {
    rt_log::line(&format!("ERROR {msg}"));
    LAST_ERROR.set(&msg);
}

/// [`fail`] for the function-reference calls, which a mod can make every frame: the line is written
/// without flushing the log to disk each time.
fn fail_unflushed(msg: String) {
    rt_log::line_unflushed(&format!("ERROR {msg}"));
    LAST_ERROR.set(&msg);
}

/// The refs `m2_lua_ref` issued, and the thread each global state's scripts run on ([`walker`] and
/// [`capture`] record it).
static REF_TABLE: Mutex<RefTable> = Mutex::new(RefTable::new());

fn ref_table() -> MutexGuard<'static, RefTable> {
    REF_TABLE.lock().expect("ref table lock")
}

fn current_thread() -> u32 {
    unsafe { GetCurrentThreadId() }
}

/// Record a failure for [`m2_lua_last_error`] (and the log) from another part of the runtime.
pub(crate) fn record_error(msg: String) {
    fail(msg);
}

/// Every captured state that is usable now.
pub(crate) fn captured_states() -> Vec<u32> {
    SEEN.iter().map(|s| s.load(Ordering::Relaxed)).filter(|&l| l != 0 && looks_like_state(l)).collect()
}

pub(crate) fn state_usable(l: u32) -> bool {
    looks_like_state(l)
}

/// A chunk of [`m2_lua_exec`] is running on this thread.
pub(crate) fn in_exec() -> bool {
    EXEC_DEPTH.with(Cell::get) > 0
}

fn sites_ready() -> &'static LuaSites {
    sites::now().expect("the Lua detours are attached only after the sites resolved")
}

fn read_u32(addr: u32) -> u32 {
    luastack::Process.read_u32(addr)
}

fn write_u32(addr: u32, v: u32) {
    unsafe { (addr as *mut u32).write_unaligned(v) }
}

/// `len` bytes from `addr` are readable, across as many adjacent regions as they span.
fn readable(addr: u32, len: u32) -> bool {
    if addr < 0x10000 {
        return false;
    }
    let mut covered = 0u32;
    while covered < len {
        let Some(at) = addr.checked_add(covered) else { return false };
        let span = luastack::Process.readable_span(at);
        if span == 0 {
            return false;
        }
        covered = covered.saturating_add(span);
    }
    true
}

/// What makes `l` not a usable `lua_State`, or `None` when it is one: its type tag at +4 is
/// `LUA_TTHREAD`, and top, base and the global state are readable, aligned, and ordered.
fn state_problem(l: u32) -> Option<String> {
    if !readable(l, 0x18) {
        return Some("the state is not readable".into());
    }
    let tt = luastack::Process.read_u8(l + 4) as u32;
    if tt != LUA_TTHREAD {
        return Some(format!("type tag {tt}, not a thread"));
    }
    let (top, base, g) = (read_u32(l + OFF_TOP), read_u32(l + OFF_BASE), read_u32(l + OFF_GLOBAL));
    let problem = if !readable(base, 16) {
        "base is not readable"
    } else if !readable(top, 4) {
        "top is not readable"
    } else if !readable(g, 4) {
        "the global state is not readable"
    } else if (top | base) & 3 != 0 {
        "top or base is misaligned"
    } else if top < base {
        "top is below base"
    } else if top - base > 0x10000 {
        "the frame is over 64 KiB"
    } else {
        return None;
    };
    Some(format!("{problem} (base {base:#x}, top {top:#x}, global {g:#x})"))
}

fn looks_like_state(l: u32) -> bool {
    state_problem(l).is_none()
}

fn new_string(l: u32, bytes: &[u8]) -> u32 {
    let f: unsafe extern "C" fn(u32, *const u8, u32) -> u32 =
        unsafe { core::mem::transmute(sites_ready().va(Site::LuaSNewLStr) as usize) };
    unsafe { f(l, bytes.as_ptr(), bytes.len() as u32) }
}

fn push(l: u32, value: u32, tag: u32) {
    let top = read_u32(l + OFF_TOP);
    write_u32(top, value);
    write_u32(top + TVALUE_TT, tag);
    write_u32(l + OFF_TOP, top + TVALUE_SIZE);
}

/// `luaL_register(L, ns.name, ns.regs)` with the engine's register ABI, then pop the table it leaves.
/// A stack slot as an offset from `L->stack`. A call that grows the stack reallocates it, so a slot is
/// held as an offset across such a call and turned back into an address after (`luaD_pcall` 0x00868AD0
/// restores its saved top as `L->stack (+0x20) + offset` for the same reason).
fn stack_off(l: u32, slot: u32) -> u32 {
    slot - read_u32(l + OFF_STACK)
}

fn stack_at(l: u32, off: u32) -> u32 {
    read_u32(l + OFF_STACK) + off
}

fn register_one(l: u32, ns: &Namespace) -> Result<(), String> {
    let func = sites_ready().va(Site::LuaLRegister);
    let top_before_off = stack_off(l, read_u32(l + OFF_TOP));
    let table = ns.regs.as_ptr() as u32;
    let name = ns.name.as_ptr() as u32;
    unsafe {
        core::arch::asm!(
            "push {table}",
            "call {func}",
            "add esp, 4",
            table = in(reg) table,
            func = in(reg) func,
            inout("ecx") l => _,
            inout("eax") name => _,
            out("edx") _,
            clobber_abi("C"),
        )
    }
    let top_before = stack_at(l, top_before_off);
    let top = read_u32(l + OFF_TOP);
    let tag = if top == top_before + TVALUE_SIZE { read_u32(top_before + TVALUE_TT) } else { u32::MAX };
    write_u32(l + OFF_TOP, top_before);
    if tag != LUA_TTABLE {
        return Err(format!(
            "luaL_register({}) on lua_State {l:#x} left the stack top at {top:#x} (was {top_before:#x}) with tag {tag:#x}; \
             expected exactly one table",
            ns.name.to_string_lossy()
        ));
    }
    Ok(())
}

/// Register namespaces `from..` on `l`; returns how many are registered on it afterwards.
fn register_from(l: u32, from: usize) -> usize {
    let list = NAMESPACES.lock().expect("namespace lock");
    for ns in list.iter().skip(from) {
        match register_one(l, ns) {
            Ok(()) => rt_log::line(&format!("registered Lua namespace {} on lua_State {l:#x}", ns.name.to_string_lossy())),
            Err(e) => fail(e),
        }
    }
    list.len()
}

/// `l`'s slot. An empty slot holds 0, and the log stub's non-Lua callers pass 0, so 0 is never a state:
/// matching it would tick the pump with no state to run on.
fn slot_of(l: u32) -> Option<usize> {
    if l == 0 {
        return None;
    }
    SEEN.iter().position(|s| s.load(Ordering::Relaxed) == l)
}

fn catch_up(slot: usize, l: u32) {
    let have = SEEN_REG[slot].load(Ordering::Acquire);
    if have < NS_COUNT.load(Ordering::Acquire) {
        SEEN_REG[slot].store(register_from(l, have), Ordering::Release);
    }
}

/// Has `l` ever been captured? Records it when not. A full table reports "seen": a missed new-cycle signal
/// costs one stale load-cycle event, a false one re-runs a cycle's work.
fn ever_seen(l: u32) -> bool {
    let n = EVER_N.load(Ordering::Acquire).min(EVER_SLOTS);
    if EVER[..n].iter().any(|e| e.load(Ordering::Relaxed) == l) {
        return true;
    }
    if n < EVER_SLOTS {
        EVER[n].store(l, Ordering::Relaxed);
        EVER_N.store(n + 1, Ordering::Release);
        return false;
    }
    true
}

fn fire<F: Fn(*mut c_void, *mut c_void)>(slots: &[Slot], count: &AtomicI32, f: F) {
    let n = (count.load(Ordering::Acquire).max(0) as usize).min(MAX_CALLBACKS);
    for s in &slots[..n] {
        let cb = s.cb.load(Ordering::Acquire);
        if !cb.is_null() {
            f(cb, s.ud.load(Ordering::Relaxed));
        }
    }
}

/// A state a script runs on: remembered, given every namespace, and announced.
fn capture(l: u32) {
    if l == CURRENT.load(Ordering::Relaxed) || l == 0 {
        return;
    }
    if slot_of(l).is_some() {
        CURRENT.store(l, Ordering::Relaxed);
        return;
    }
    if !looks_like_state(l) {
        return;
    }
    CURRENT.store(l, Ordering::Relaxed);
    ref_table().state_seen(read_u32(l + OFF_GLOBAL), current_thread());
    let slot = SEEN_NEXT.fetch_add(1, Ordering::Relaxed) % SEEN_SLOTS;
    SEEN[slot].store(l, Ordering::Relaxed);
    SEEN_REG[slot].store(0, Ordering::Release);
    let is_new = !ever_seen(l);
    rt_log::line(&format!("Lua VM captured via type(): L={l:#x}{}", if is_new { " (new)" } else { "" }));
    catch_up(slot, l);
    crate::debug::state_captured(l);
    fire(&STATE_CBS, &STATE_N, |cb, ud| {
        let cb: StateCb = unsafe { core::mem::transmute(cb) };
        unsafe { cb(l as *mut c_void, c_int::from(is_new), ud) };
    });
}

/// Ticks that carried a state, ticks with none, and ticks from the frame detour: what tells a quiet Lua
/// from a stalled pump. `M2Live.Ticks` reads them.
pub(crate) static TICKS_STATE: AtomicU32 = AtomicU32::new(0);
pub(crate) static TICKS_NULL: AtomicU32 = AtomicU32::new(0);
pub(crate) static TICKS_FRAME: AtomicU32 = AtomicU32::new(0);

/// One game-thread call on `l` from a detour.
pub(crate) fn tick(l: u32) {
    if l == 0 {
        TICKS_NULL.fetch_add(1, Ordering::Relaxed);
        return;
    }
    TICKS_STATE.fetch_add(1, Ordering::Relaxed);
    if EXEC_DEPTH.with(Cell::get) == 0 && slot_of(l).is_some() {
        crate::debug::tick(l);
    }
    let ticking = TICK_N.load(Ordering::Relaxed) > 0;
    if !ticking && !REG_PENDING.load(Ordering::Relaxed) {
        return;
    }
    if EXEC_DEPTH.with(Cell::get) > 0 {
        return;
    }
    let Some(slot) = slot_of(l) else { return };
    if REG_PENDING.load(Ordering::Acquire) {
        catch_up(slot, l);
        let n = NS_COUNT.load(Ordering::Acquire);
        let all = SEEN.iter().zip(&SEEN_REG).all(|(s, r)| s.load(Ordering::Relaxed) == 0 || r.load(Ordering::Acquire) >= n);
        if all {
            REG_PENDING.store(false, Ordering::Release);
        }
    }
    if ticking {
        fire(&TICK_CBS, &TICK_N, |cb, ud| {
            let cb: TickCb = unsafe { core::mem::transmute(cb) };
            unsafe { cb(l as *mut c_void, ud) };
        });
    }
}

unsafe extern "stdcall" fn walker(host: u32) {
    let orig: unsafe extern "stdcall" fn(u32) = unsafe { core::mem::transmute(ORIG_WALKER.load(Ordering::Acquire)) };
    unsafe { orig(host) };
    let l = read_u32(host + HOST_LUA_STATE);
    if l == 0 {
        fail(format!("the script host {host:#x} has no lua_State at +{HOST_LUA_STATE:#x} after its namespaces were registered"));
        return;
    }
    // A state the host just built: a new life of its global state, whose registry holds no earlier ref.
    let g = read_u32(l + OFF_GLOBAL);
    let held = ref_table().state_created(g, current_thread());
    if held > 0 {
        rt_log::line(&format!("global state {g:#x} was rebuilt; {held} ref(s) taken before it are no longer usable"));
    }
    register_from(l, 0);
}

unsafe extern "C" fn type_detour(l: *mut c_void) -> c_int {
    capture(l as u32);
    tick(l as u32);
    let orig: LuaCFunction = unsafe { core::mem::transmute(ORIG_TYPE.load(Ordering::Acquire)) };
    unsafe { orig(l) }
}

unsafe extern "C" fn widget_detour(l: *mut c_void) -> c_int {
    tick(l as u32);
    let orig: LuaCFunction = unsafe { core::mem::transmute(ORIG_WIDGET.load(Ordering::Acquire)) };
    unsafe { orig(l) }
}

/// Once a frame, from RunFrame: a tick that does not depend on the game's Lua reaching a detour. The
/// script host's state, and only while no Lua runs on it: a frame's start has none on the stack, and a
/// frame entered from inside Lua would be a chunk's stack under a running function.
unsafe extern "C" fn frame_detour() {
    TICKS_FRAME.fetch_add(1, Ordering::Relaxed);
    let host = sites_ready().host();
    if luastack::Process.read_u8(host + HOST_INITIALISED) != 0 {
        let l = read_u32(host + HOST_LUA_STATE);
        if looks_like_state(l) && read_u32(l + OFF_CI) == read_u32(l + OFF_BASE_CI) {
            tick(l);
        }
    }
    let orig: unsafe extern "C" fn() = unsafe { core::mem::transmute(ORIG_FRAME.load(Ordering::Acquire)) };
    unsafe { orig() }
}

fn attach(s: &LuaSites, site: Site, detour: *mut c_void, orig: &AtomicUsize) -> Result<(), String> {
    let at = s.va(site);
    let first = luastack::Process.read_u8(at);
    if first == 0xe9 {
        return Err(format!("{} at {at:#x} is already detoured by another module (it starts with a jmp)", site.name()));
    }
    if !unsafe { hook::attach(at as *mut c_void, detour, orig.as_ptr().cast()) } {
        return Err(format!("MinHook could not detour {} at {at:#x}", site.name()));
    }
    rt_log::line(&format!("detoured {} at {at:#x}", site.name()));
    Ok(())
}

fn install() -> Result<(), String> {
    let mut installed = INSTALLED.lock().expect("install lock");
    if *installed {
        return Ok(());
    }
    let s = sites::wait()?;
    // Here, on the mod's thread, so the scan never stalls the game thread. A miss fails the install before
    // any detour is attached (m2_lua_install logs it as ERROR), and every ref call returns it too.
    sites::resolve_refs()?;
    attach(s, Site::RegisterNamespaces, walker as *mut c_void, &ORIG_WALKER)?;
    attach(s, Site::LuaBType, type_detour as *mut c_void, &ORIG_TYPE)?;
    attach(s, Site::CreateTextWidget, widget_detour as *mut c_void, &ORIG_WIDGET)?;
    attach(s, Site::FrameStep, frame_detour as *mut c_void, &ORIG_FRAME)?;
    stub::TICK_ON.store(true, Ordering::Release);
    stub::attach(s)?;
    crate::debug::install_detours();
    *installed = true;
    rt_log::line("Lua host installed");
    crate::live::register();
    Ok(())
}

/// Resolve the sites (waiting up to 30 s for SecuROM to decrypt the game's code), then the function-reference
/// sites, and attach the Lua host's detours. Idempotent. 1 on success; 0 with [`m2_lua_last_error`] set when
/// a site did not resolve (a function-reference pattern included: nothing is attached then) or a detour
/// could not be attached. Not from `DllMain`.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_install() -> c_int {
    match install() {
        Ok(()) => 1,
        Err(e) => {
            fail(format!("m2_lua_install: {e}"));
            0
        }
    }
}

/// The message of the last failure the Lua host recorded on the calling thread, or NULL. Valid until that
/// thread's next failure; another thread's failure never touches it.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_last_error() -> *const c_char {
    LAST_ERROR.get()
}

/// Add the functions `fns` to the global table `name` on every Lua state the game creates or a script runs
/// on, creating the table when it does not exist. Several calls, from one mod or several, may add to the same
/// table (the engine's own `Net`, say). `fns` ends with a NULL name; both are copied. 0 for a NULL or empty
/// name, a NULL table, a function already added to that table or named twice in `fns`, or more than 32 calls
/// or 256 functions.
///
/// # Safety
/// `name` is NULL or a NUL-terminated string; `fns` is NULL or a NULL-name-terminated array whose names
/// are NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_add_namespace(name: *const c_char, fns: *const LuaReg) -> c_int {
    if name.is_null() || fns.is_null() {
        fail("m2_lua_add_namespace: NULL name or table".into());
        return 0;
    }
    let ns_name = unsafe { CStr::from_ptr(name) }.to_owned();
    if ns_name.is_empty() {
        fail("m2_lua_add_namespace: empty name".into());
        return 0;
    }
    let mut names = Vec::new();
    let mut funcs = Vec::new();
    for i in 0.. {
        let row = unsafe { &*fns.add(i) };
        if row.name.is_null() {
            break;
        }
        if i == MAX_FUNCTIONS {
            fail(format!("m2_lua_add_namespace({}): more than {MAX_FUNCTIONS} functions", ns_name.to_string_lossy()));
            return 0;
        }
        let Some(f) = row.func else {
            fail(format!("m2_lua_add_namespace({}): row {i} has no function", ns_name.to_string_lossy()));
            return 0;
        };
        names.push(unsafe { CStr::from_ptr(row.name) }.to_owned());
        funcs.push(f);
    }
    let mut regs: Vec<LuaReg> = names.iter().zip(&funcs).map(|(n, f)| LuaReg { name: n.as_ptr(), func: Some(*f) }).collect();
    regs.push(LuaReg { name: core::ptr::null(), func: None });
    let mut list = NAMESPACES.lock().expect("namespace lock");
    let earlier: Vec<(&CStr, &CStr)> = list.iter().flat_map(|n| n.names.iter().map(|f| (n.name.as_c_str(), f.as_c_str()))).collect();
    let fields: Vec<&CStr> = names.iter().map(|f| f.as_c_str()).collect();
    if let Some(why) = namespace_add_refused(&earlier, &ns_name, &fields) {
        drop(list);
        fail(format!("m2_lua_add_namespace: {why}"));
        return 0;
    }
    if list.len() == MAX_NAMESPACES {
        drop(list);
        fail(format!("m2_lua_add_namespace({}): {MAX_NAMESPACES} namespaces are already added", ns_name.to_string_lossy()));
        return 0;
    }
    rt_log::line(&format!("namespace {} added ({} functions)", ns_name.to_string_lossy(), funcs.len()));
    list.push(Namespace { name: ns_name, names, regs });
    NS_COUNT.store(list.len(), Ordering::Release);
    REG_PENDING.store(true, Ordering::Release);
    1
}

fn claim(slots: &[Slot], count: &AtomicI32, cb: *mut c_void, ud: *mut c_void) -> c_int {
    let n = count.fetch_add(1, Ordering::AcqRel);
    let Some(s) = usize::try_from(n).ok().and_then(|i| slots.get(i)) else { return 0 };
    s.ud.store(ud, Ordering::Relaxed);
    s.cb.store(cb, Ordering::Release);
    1
}

/// Call `cb(L, is_new_cycle, ud)` on the game thread for each state a script runs on that has not been
/// seen recently; `is_new_cycle` is 1 when this process has never seen that state. 0 when full (16).
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_on_state(cb: Option<StateCb>, ud: *mut c_void) -> c_int {
    match cb {
        Some(cb) => claim(&STATE_CBS, &STATE_N, cb as *mut c_void, ud),
        None => 0,
    }
}

/// Call `cb(L, ud)` on the game thread from each detoured call on a captured state, except while a chunk
/// or an [`m2_lua_pcall`] runs on that thread. Very frequent: keep the callback cheap. 0 when full (16).
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_on_tick(cb: Option<TickCb>, ud: *mut c_void) -> c_int {
    match cb {
        Some(cb) => claim(&TICK_CBS, &TICK_N, cb as *mut c_void, ud),
        None => 0,
    }
}

/// 1 when `L` is a usable `lua_State` right now (the check [`m2_lua_exec`] makes before running), else 0.
/// A tick's `L` can be a state that has since ended: check before taking work off a queue for it.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_state_ok(l: *mut c_void) -> c_int {
    match state_problem(l as u32) {
        None => 1,
        Some(problem) => {
            note_rejected(l as u32, &problem);
            0
        }
    }
}

/// Log each kind of rejection once: a caller that gets 0 only skips the state, so the reason would
/// otherwise be lost, and a stalled bridge cannot say why.
fn note_rejected(l: u32, problem: &str) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let kind = problem.split(" (").next().unwrap_or(problem);
    let mut seen = SEEN.lock().expect("rejection list lock");
    if !seen.iter().any(|k| k == kind) {
        seen.push(kind.to_string());
        rt_log::line(&format!("lua_State {l:#x} rejected: {problem}"));
    }
}

/// 1 while [`m2_lua_exec`] or [`m2_lua_pcall`] runs on this thread.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_in_exec() -> c_int {
    c_int::from(EXEC_DEPTH.with(Cell::get) > 0)
}

struct ExecGuard;
impl ExecGuard {
    fn enter() -> Self {
        EXEC_DEPTH.with(|d| d.set(d.get() + 1));
        ExecGuard
    }
}
impl Drop for ExecGuard {
    fn drop(&mut self) {
        EXEC_DEPTH.with(|d| d.set(d.get() - 1));
    }
}

fn exec(l: u32, code: &[u8]) -> (c_int, Vec<u8>) {
    let refuse = |m: &str| (EXEC_REFUSED, m.as_bytes().to_vec());
    let Ok(s) = sites::now() else { return refuse("[bridge] the game's Lua sites are not resolved") };
    if let Some(problem) = state_problem(l) {
        return refuse(&format!("[bridge] L={l:#x} failed validation: {problem}"));
    }
    if code.is_empty() {
        return refuse("[bridge] empty chunk");
    }
    if code.len() >= MAX_CHUNK {
        return refuse("[bridge] chunk too large");
    }
    // Every slot below is held as an offset from L->stack: loadstring and pcall can grow the stack, which
    // moves it, and an address taken before either call would then name freed memory.
    let saved_top = read_u32(l + OFF_TOP);
    let saved_base = read_u32(l + OFF_BASE);
    if !readable(saved_top, 3 * TVALUE_SIZE) {
        return refuse("[bridge] L->top + scratch unwritable");
    }
    let stack_before = read_u32(l + OFF_STACK);
    let nb_off = stack_off(l, saved_top);
    let base_off = stack_off(l, saved_base);
    let frame_bytes = (saved_top - saved_base).min(FRAME_SAVE_MAX);
    let frame: Option<Vec<u8>> = (frame_bytes > 0 && readable(saved_base, frame_bytes))
        .then(|| (0..frame_bytes).map(|i| luastack::Process.read_u8(saved_base + i)).collect());
    let restore_frame = |f: &Option<Vec<u8>>| {
        if let Some(bytes) = f {
            let base = stack_at(l, base_off);
            for (i, b) in bytes.iter().enumerate() {
                unsafe { ((base + i as u32) as *mut u8).write(*b) };
            }
        }
    };
    let restore_stack = || {
        write_u32(l + OFF_TOP, stack_at(l, nb_off));
        write_u32(l + OFF_BASE, stack_at(l, base_off));
    };
    let _guard = ExecGuard::enter();
    let source = new_string(l, code);
    let nb = stack_at(l, nb_off);
    write_u32(nb, source);
    write_u32(nb + TVALUE_TT, LUA_TSTRING);
    write_u32(l + OFF_BASE, nb);
    write_u32(l + OFF_TOP, nb + TVALUE_SIZE);
    let loadstring: LuaCFunction = unsafe { core::mem::transmute(s.va(Site::LuaBLoadString) as usize) };
    let load_n = unsafe { loadstring(l as *mut c_void) };
    let nb = stack_at(l, nb_off);
    if load_n == 2 {
        let text = lua_exec::render_compile_error(&luastack::Process, nb + 2 * TVALUE_SIZE);
        restore_stack();
        return (EXEC_COMPILE, text);
    }
    if load_n != 1 {
        restore_stack();
        return refuse(&format!("[bridge] loadstring returned {load_n}"));
    }
    let top_slot = read_u32(l + OFF_TOP) - TVALUE_SIZE;
    let compiled_tt = read_u32(top_slot + TVALUE_TT);
    if compiled_tt != LUA_TFUNCTION {
        restore_stack();
        return refuse(&format!("[bridge] loadstring result not a function (tt={compiled_tt})"));
    }
    if top_slot != nb {
        write_u32(nb, read_u32(top_slot));
        write_u32(nb + TVALUE_TT, compiled_tt);
    }
    write_u32(l + OFF_TOP, nb + TVALUE_SIZE);
    let pcall: LuaCFunction = unsafe { core::mem::transmute(s.va(Site::LuaBPcall) as usize) };
    unsafe { pcall(l as *mut c_void) };
    let stack_after = read_u32(l + OFF_STACK);
    if stack_after != stack_before {
        rt_log::line(&format!(
            "lua_State {l:#x}: the chunk grew the Lua stack, which moved from {stack_before:#x} to {stack_after:#x}"
        ));
    }
    let nb = stack_at(l, nb_off);
    let saved_base = stack_at(l, base_off);
    let saved_status = (frame.is_some() && frame_bytes >= TVALUE_SIZE)
        .then(|| (read_u32(saved_base + TVALUE_TT), read_u32(saved_base)));
    restore_frame(&frame);
    let slots = ((read_u32(l + OFF_TOP).saturating_sub(nb)) / TVALUE_SIZE) as usize;
    let slots = slots.min(MAX_RESULTS);
    let slot0 = (slots >= 1).then(|| (read_u32(nb + TVALUE_TT), read_u32(nb)));
    let o = lua_exec::outcome(saved_status, slot0);
    let text = lua_exec::render(&luastack::Process, o, nb, slots);
    restore_stack();
    (if o.succeeded { EXEC_OK } else { EXEC_RUNTIME }, text)
}

/// Run `len` bytes of Lua source on `L` and write the result line to `out` (NUL-terminated, truncated to
/// `cap`): `[ok]\t…` / `[runtime] …` / `[compile] "…"` / `[bridge] …`. Returns 0 ok, 1 compile error,
/// 2 runtime error, 3 refused. Call it on the game thread from a state or tick callback, or from a Lua C
/// function, with the `L` it was given.
///
/// # Safety
/// `chunk` is readable for `len` bytes; `out` is NULL or writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_exec(l: *mut c_void, chunk: *const c_char, len: c_int, out: *mut c_char, cap: c_int) -> c_int {
    let (status, text) = if chunk.is_null() || len < 0 {
        (EXEC_REFUSED, b"[bridge] no chunk".to_vec())
    } else {
        exec(l as u32, unsafe { core::slice::from_raw_parts(chunk.cast::<u8>(), len as usize) })
    };
    if !out.is_null() && cap > 0 {
        let n = text.len().min(cap as usize - 1);
        unsafe {
            core::ptr::copy_nonoverlapping(text.as_ptr(), out.cast::<u8>(), n);
            *out.add(n) = 0;
        }
    }
    status
}

/// Raise a Lua error with `msg` from inside a Lua C function; does not return (`luaG_errormsg` `longjmp`s
/// to the enclosing `pcall`). Frames between hold nothing that needs dropping.
///
/// # Safety
/// `l` is the running state of a Lua C function call; `msg` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_raise(l: *mut c_void, msg: *const c_char) -> ! {
    let l = l as u32;
    let bytes: &[u8] = if msg.is_null() { b"error" } else { unsafe { CStr::from_ptr(msg) }.to_bytes() };
    push(l, new_string(l, bytes), LUA_TSTRING);
    let errormsg = sites_ready().va(Site::LuaGErrorMsg);
    unsafe { core::arch::asm!("call {f}", f = in(reg) errormsg, in("eax") l, options(noreturn)) }
}

#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_push_nil(l: *mut c_void) {
    push(l as u32, 0, LUA_TNIL);
}

#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_push_bool(l: *mut c_void, b: c_int) {
    push(l as u32, u32::from(b != 0), LUA_TBOOLEAN);
}

/// Pushed as the game's `lua_Number`, a 32-bit float.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_push_number(l: *mut c_void, n: f64) {
    push(l as u32, (n as f32).to_bits(), LUA_TNUMBER);
}

/// An interned string (`luaS_newlstr`), so `==` compares it with any other string of the same bytes.
/// `len < 0` takes `strlen(s)`.
///
/// # Safety
/// `s` is readable for `len` bytes, or NUL-terminated when `len < 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_push_string(l: *mut c_void, s: *const c_char, len: c_int) {
    let bytes: &[u8] = if s.is_null() {
        b""
    } else if len < 0 {
        unsafe { CStr::from_ptr(s) }.to_bytes()
    } else {
        unsafe { core::slice::from_raw_parts(s.cast::<u8>(), len as usize) }
    };
    let l = l as u32;
    push(l, new_string(l, bytes), LUA_TSTRING);
}

/// `L` is a usable state, or why not.
fn usable(l: u32) -> Result<(), String> {
    match state_problem(l) {
        None => Ok(()),
        Some(problem) => Err(format!("lua_State {l:#x} is not usable: {problem}")),
    }
}

/// `L` is usable and the calling thread is the one its global state's scripts run on; its global state.
fn on_owner_thread(l: u32) -> Result<u32, String> {
    usable(l)?;
    let g = read_u32(l + OFF_GLOBAL);
    ref_table().check_thread(g, current_thread())?;
    Ok(g)
}

/// `luaL_ref(L, LUA_REGISTRYINDEX)` on the value at the top: `L` in `EAX`, `t` pushed, the ref in `EAX`.
fn call_lual_ref(f: u32, l: u32) -> c_int {
    let r: u32;
    unsafe {
        core::arch::asm!(
            "push {t}",
            "call {f}",
            "add esp, 4",
            t = const LUA_REGISTRYINDEX,
            f = in(reg) f,
            inout("eax") l => r,
            out("ecx") _,
            out("edx") _,
            clobber_abi("C"),
        )
    }
    r as c_int
}

/// `luaL_unref(L, LUA_REGISTRYINDEX, ref)`: `L` in `EDI`, `ref` pushed (the registry is folded into the
/// game's function).
fn call_lual_unref(f: u32, l: u32, r: c_int) {
    unsafe {
        core::arch::asm!(
            "push {r}",
            "call {f}",
            "add esp, 4",
            r = in(reg) r,
            f = in(reg) f,
            inout("edi") l => _,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
            clobber_abi("C"),
        )
    }
}

/// `lua_rawgeti(L, LUA_REGISTRYINDEX, ref)`: `L` in `EDI`, the index in `ECX`, `ref` pushed.
fn call_lua_rawgeti(f: u32, l: u32, r: c_int) {
    unsafe {
        core::arch::asm!(
            "push {r}",
            "call {f}",
            "add esp, 4",
            r = in(reg) r,
            f = in(reg) f,
            inout("edi") l => _,
            inout("ecx") LUA_REGISTRYINDEX => _,
            out("eax") _,
            out("edx") _,
            clobber_abi("C"),
        )
    }
}

/// `lua_pcall(L, nargs, nresults, 0)`: `L` in `EAX`, `errfunc` in `ECX`, `nresults` in `EDI`, `nargs`
/// pushed, the status in `EAX`.
fn call_lua_pcall(f: u32, l: u32, nargs: c_int, nresults: c_int) -> c_int {
    let status: u32;
    unsafe {
        core::arch::asm!(
            "push {n}",
            "call {f}",
            "add esp, 4",
            n = in(reg) nargs,
            f = in(reg) f,
            inout("eax") l => status,
            inout("ecx") 0u32 => _,
            inout("edi") nresults => _,
            out("edx") _,
            clobber_abi("C"),
        )
    }
    status as c_int
}

fn lua_ref(l: u32, idx: c_int) -> Result<c_int, String> {
    let s = sites::refs()?;
    let g = on_owner_thread(l)?;
    let slot = lua_stack::stack_slot(&luastack::Process, l, idx)?;
    if !lua_stack::has_room(&luastack::Process, l, 2)? {
        return Err(format!("lua_State {l:#x} has no room for the copy and luaL_ref's free-list read"));
    }
    let tt = read_u32(slot + TVALUE_TT);
    push(l, read_u32(slot), tt);
    let r = call_lual_ref(s.lual_ref, l);
    if r <= 0 {
        return Err(format!("luaL_ref returned {r} for the {} at stack index {idx}", type_name(tt as i32)));
    }
    ref_table().issue(g, r).map_err(|e| {
        // luaL_ref popped the copy, so the room checked above holds luaL_unref's free-list read.
        call_lual_unref(s.lual_unref, l, r);
        format!("{e}; registry slot {r} released")
    })
}

/// Keep a reference to the value at Lua stack index `idx` of `L` (1 is the first value of the running
/// function's frame, -1 the top) in the registry: `luaL_ref(L, LUA_REGISTRYINDEX)` on a copy, so the stack
/// is unchanged. The ref (> 0) is a handle of the runtime's own for that registry slot, usable on `L`'s
/// global state until it is released or the state is rebuilt. 0 when there is none to take (a nil value,
/// an index naming no value, an unusable state, a call off the state's thread, the sites unresolved),
/// logged and kept for [`m2_lua_last_error`].
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_ref(l: *mut c_void, idx: c_int) -> c_int {
    match lua_ref(l as u32, idx) {
        Ok(r) => r,
        Err(e) => {
            fail_unflushed(format!("m2_lua_ref: {e}"));
            0
        }
    }
}

fn lua_unref(l: u32, r: c_int) -> Result<(), String> {
    let s = sites::refs()?;
    let g = on_owner_thread(l)?;
    let lua_ref = {
        let mut table = ref_table();
        table.lua_ref(g, r).inspect_err(|_| {
            table.forget_earlier_life(r);
        })?
    };
    if !lua_stack::has_room(&luastack::Process, l, 1)? {
        return Err(format!("lua_State {l:#x} has no room for luaL_unref's free-list read"));
    }
    call_lual_unref(s.lual_unref, l, lua_ref);
    ref_table().release(r);
    Ok(())
}

/// Release `ref` (`luaL_unref(L, LUA_REGISTRYINDEX, <its slot>)`); the registry reuses the slot. 1 when
/// released; 0 when refused (a ref not live on `L`'s global state: never issued, already released, taken
/// on another global state or before this one was rebuilt; an unusable state; a call off the state's
/// thread; the sites unresolved), logged and kept for [`m2_lua_last_error`]. A ref from before a rebuild
/// is forgotten as it is refused: its registry is gone, so there is nothing to release.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_unref(l: *mut c_void, r: c_int) -> c_int {
    match lua_unref(l as u32, r) {
        Ok(()) => 1,
        Err(e) => {
            fail_unflushed(format!("m2_lua_unref: {e}"));
            0
        }
    }
}

fn lua_push_ref(l: u32, r: c_int) -> Result<(), String> {
    let s = sites::refs()?;
    let g = on_owner_thread(l)?;
    let lua_ref = ref_table().lua_ref(g, r)?;
    if !lua_stack::has_room(&luastack::Process, l, 1)? {
        return Err(format!("lua_State {l:#x} has no room for the value"));
    }
    call_lua_rawgeti(s.lua_rawgeti, l, lua_ref);
    Ok(())
}

/// Push the value `ref` holds (`lua_rawgeti(L, LUA_REGISTRYINDEX, <its slot>)`). 1 when pushed; 0 when
/// nothing was pushed (a ref not live on `L`'s global state, no stack room, an unusable state, a call off
/// the state's thread, the sites unresolved), logged and kept for [`m2_lua_last_error`].
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_push_ref(l: *mut c_void, r: c_int) -> c_int {
    match lua_push_ref(l as u32, r) {
        Ok(()) => 1,
        Err(e) => {
            fail_unflushed(format!("m2_lua_push_ref: {e}"));
            0
        }
    }
}

fn lua_pcall(l: u32, nargs: c_int, nresults: c_int) -> Result<c_int, String> {
    let s = sites::refs()?;
    on_owner_thread(l)?;
    let callee = lua_stack::pcall_callee(&luastack::Process, l, nargs, nresults)?;
    let callee_tt = read_u32(callee + TVALUE_TT);
    let _guard = ExecGuard::enter();
    let status = call_lua_pcall(s.lua_pcall, l, nargs, nresults);
    if status != 0 {
        let top = read_u32(l + OFF_TOP) - TVALUE_SIZE;
        // At most a log line of the message: the log cuts the line there, and a script can raise megabytes.
        let mut msg = Vec::new();
        lua_exec::format_value_max(&luastack::Process, top, &mut msg, crate::log::LINE_MAX as u32);
        write_u32(l + OFF_TOP, top);
        fail_unflushed(format!(
            "m2_lua_pcall: calling the {} failed with Lua status {status}: {}",
            type_name(callee_tt as i32),
            String::from_utf8_lossy(&msg)
        ));
    }
    Ok(status)
}

/// `lua_pcall(L, nargs, nresults, 0)`: call the function at `-(nargs + 1)` with the `nargs` values above
/// it, which are popped with it. 0 when it returned, with `nresults` results pushed from the function's
/// slot (all of them for `LUA_MULTRET`, -1); else the Lua status (2 a runtime error, 4 out of memory, 5 an
/// error in the error handler), with the error logged and kept for [`m2_lua_last_error`], and popped. -1
/// when it refused to call (an unusable state, a call off the state's thread, a frame without the function
/// and its arguments, `nresults` below -1 or more results than fit below `ci->top`, the sites unresolved):
/// nothing was called and the stack is unchanged. Ticks do not run on this thread while the call does, and
/// [`m2_lua_in_exec`] reports 1.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_pcall(l: *mut c_void, nargs: c_int, nresults: c_int) -> c_int {
    match lua_pcall(l as u32, nargs, nresults) {
        Ok(status) => status,
        Err(e) => {
            fail_unflushed(format!("m2_lua_pcall: {e}"));
            PCALL_REFUSED
        }
    }
}

fn lua_pop(l: u32, n: c_int) -> Result<(), String> {
    on_owner_thread(l)?;
    let top = lua_stack::pop_top(&luastack::Process, l, n)?;
    write_u32(l + OFF_TOP, top);
    Ok(())
}

/// `lua_pop(L, n)`: remove the top `n` values, as Lua 5.1's `lua_settop(L, -n-1)` does, by lowering
/// `L->top` (`lua_stack::pop_top`). 1 when removed; 0 when refused (n < 0, more than the values of the
/// running function's frame, an unusable state, a call off the state's thread), with nothing removed,
/// logged and kept for [`m2_lua_last_error`]. The way to take back what a refused [`m2_lua_push_ref`] or
/// [`m2_lua_pcall`] left on the stack.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_pop(l: *mut c_void, n: c_int) -> c_int {
    match lua_pop(l as u32, n) {
        Ok(()) => 1,
        Err(e) => {
            fail_unflushed(format!("m2_lua_pop: {e}"));
            0
        }
    }
}
