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
//!
//! Every detoured function is a cdecl `lua_CFunction` except the walker (`__stdcall (host)`).

use crate::{hook, luastack, rt_log, sites, stub};
use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use mercs2_formats::lua_exec::{self, MAX_CHUNK, MAX_RESULTS};
use mercs2_formats::lua_sites::{HOST_LUA_STATE, LuaSites, Site};
use mercs2_formats::lua_stack::{
    LUA_TBOOLEAN, LUA_TFUNCTION, LUA_TNIL, LUA_TNUMBER, LUA_TSTRING, LUA_TTABLE, LUA_TTHREAD, Mem, OFF_BASE, OFF_GLOBAL,
    OFF_TOP, TVALUE_SIZE, TVALUE_TT,
};
use std::ffi::CString;
use std::sync::Mutex;

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
    _names: Vec<CString>,
    regs: Vec<LuaReg>,
}
// SAFETY: the rows point into `_names`, owned by the same value, and at functions; read only under the lock.
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
static LAST_ERROR: Mutex<Option<CString>> = Mutex::new(None);
static ORIG_WALKER: AtomicUsize = AtomicUsize::new(0);
static ORIG_TYPE: AtomicUsize = AtomicUsize::new(0);
static ORIG_WIDGET: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static EXEC_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn fail(msg: String) {
    rt_log::line(&format!("ERROR {msg}"));
    let c = CString::new(msg.replace('\0', " ")).expect("NULs replaced");
    *LAST_ERROR.lock().expect("last error lock") = Some(c);
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
fn register_one(l: u32, ns: &Namespace) -> Result<(), String> {
    let func = sites_ready().va(Site::LuaLRegister);
    let top_before = read_u32(l + OFF_TOP);
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

fn slot_of(l: u32) -> Option<usize> {
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
    let slot = SEEN_NEXT.fetch_add(1, Ordering::Relaxed) % SEEN_SLOTS;
    SEEN[slot].store(l, Ordering::Relaxed);
    SEEN_REG[slot].store(0, Ordering::Release);
    let is_new = !ever_seen(l);
    rt_log::line(&format!("Lua VM captured via type(): L={l:#x}{}", if is_new { " (new)" } else { "" }));
    catch_up(slot, l);
    fire(&STATE_CBS, &STATE_N, |cb, ud| {
        let cb: StateCb = unsafe { core::mem::transmute(cb) };
        unsafe { cb(l as *mut c_void, c_int::from(is_new), ud) };
    });
}

/// One game-thread call on `l` from a detour.
pub(crate) fn tick(l: u32) {
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
    attach(s, Site::RegisterNamespaces, walker as *mut c_void, &ORIG_WALKER)?;
    attach(s, Site::LuaBType, type_detour as *mut c_void, &ORIG_TYPE)?;
    attach(s, Site::CreateTextWidget, widget_detour as *mut c_void, &ORIG_WIDGET)?;
    stub::TICK_ON.store(true, Ordering::Release);
    stub::attach(s)?;
    *installed = true;
    rt_log::line("Lua host installed");
    Ok(())
}

/// Resolve the sites (waiting up to 30 s for SecuROM to decrypt the game's code) and attach the Lua host's
/// detours. Idempotent. 1 on success; 0 with [`m2_lua_last_error`] set. Not from `DllMain`.
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

/// The message of the last failure the Lua host recorded, or NULL. Valid until the next failure.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_last_error() -> *const c_char {
    LAST_ERROR.lock().expect("last error lock").as_ref().map_or(core::ptr::null(), |c| c.as_ptr())
}

/// Add a global table `name` of functions to every Lua state the game creates or a script runs on.
/// `fns` ends with a NULL name; both are copied. 0 for a NULL or empty name, a NULL table, a name already
/// added, or more than 32 namespaces or 256 functions.
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
    if list.iter().any(|n| n.name == ns_name) {
        drop(list);
        fail(format!("m2_lua_add_namespace: {} is already added", ns_name.to_string_lossy()));
        return 0;
    }
    if list.len() == MAX_NAMESPACES {
        drop(list);
        fail(format!("m2_lua_add_namespace({}): {MAX_NAMESPACES} namespaces are already added", ns_name.to_string_lossy()));
        return 0;
    }
    rt_log::line(&format!("namespace {} added ({} functions)", ns_name.to_string_lossy(), funcs.len()));
    list.push(Namespace { name: ns_name, _names: names, regs });
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
/// runs on that thread. Very frequent: keep the callback cheap. 0 when full (16).
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
    c_int::from(looks_like_state(l as u32))
}

/// 1 while [`m2_lua_exec`] runs on this thread.
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
    let saved_top = read_u32(l + OFF_TOP);
    let saved_base = read_u32(l + OFF_BASE);
    let nb = saved_top;
    if !readable(nb, 3 * TVALUE_SIZE) {
        return refuse("[bridge] L->top + scratch unwritable");
    }
    let frame_bytes = (nb - saved_base).min(FRAME_SAVE_MAX);
    let frame: Option<Vec<u8>> = (frame_bytes > 0 && readable(saved_base, frame_bytes))
        .then(|| (0..frame_bytes).map(|i| luastack::Process.read_u8(saved_base + i)).collect());
    let restore_frame = |f: &Option<Vec<u8>>| {
        if let Some(bytes) = f {
            for (i, b) in bytes.iter().enumerate() {
                unsafe { ((saved_base + i as u32) as *mut u8).write(*b) };
            }
        }
    };
    let restore_stack = || {
        write_u32(l + OFF_TOP, saved_top);
        write_u32(l + OFF_BASE, saved_base);
    };
    let _guard = ExecGuard::enter();
    let source = new_string(l, code);
    write_u32(nb, source);
    write_u32(nb + TVALUE_TT, LUA_TSTRING);
    write_u32(l + OFF_BASE, nb);
    write_u32(l + OFF_TOP, nb + TVALUE_SIZE);
    let loadstring: LuaCFunction = unsafe { core::mem::transmute(s.va(Site::LuaBLoadString) as usize) };
    let load_n = unsafe { loadstring(l as *mut c_void) };
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
