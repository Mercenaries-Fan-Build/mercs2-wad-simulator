//! The Lua debugger in the game (`m2/m2_lua_debug.h`): the retail host of `mercs2_formats::lua_debug`.
//!
//! What to report, when to pause and what each queued request does are decided in `mercs2_formats`
//! (`lua_debug::session`, `lua_debug::retail`, `lua_debug::host`) over process memory; this module installs
//! the hook, carries requests to the game thread, delivers frames through the one sink a transport
//! registers (the lua-bridge Shipment), and blocks the game thread while paused.
//!
//! * **Requests** arrive on the transport's threads ([`m2_lua_debug_request`]), are parsed there and queued;
//!   the game thread handles them from the Lua host's tick, or from the pause loop while paused.
//! * **The hook** is written into each captured `lua_State` at the fields `luaV_execute` (`0x866950`) and
//!   `luaD_callhook` (`0x868290`) read: `hookmask` +0x36, `basehookcount` +0x38, `hookcount` +0x3C, `hook`
//!   +0x40. The mask is call | return, plus count with a count of 1 only while the running frame raises
//!   line events (`Retail` decides at attach and on every call and return). It is called cdecl
//!   `hook(L, &ar)` with `ar.event` at +0 and `ar.i_ci` at +0x60. Nothing is reported while a chunk of
//!   [`crate::lua_host`]'s executor runs (a bridge chunk is not the game's code), and the VM does not call
//!   the hook inside the hook. The game can close a state and build a new one at the same address (it does
//!   when the script host starts again), and the new state carries no hook: every tick on a state checks
//!   for the hook while attached and puts it back, logged.
//! * **`luaU_undump`** is detoured to stamp every chunk the game loads (`lua_debug::stamp`), so a module
//!   loaded before a client attached is reported with its stamp at attach.
//! * **`luaD_poscall`** (L in `ECX`, first result in `EAX`) is detoured to read an engine binding's results,
//!   which the return hook cannot see.
//! * **Engine binding names** (`debug.bindings`, and what an engine breakpoint can name) are the running
//!   executable's namespaced `luaL_Reg` rows (`mercs2_exemap::luareg`), read once on the transport thread
//!   that brings the first request, before the debugger exists.
//! * **A filter change** (`debug.watch`) decides each hooked state's count hook again for its running frame.
//! * **Pausing** blocks the game thread in the hook: on a line breakpoint or a step from the count hook, and
//!   on an engine breakpoint from the call hook, before the binding runs. The game cannot draw, so a topmost notice window on its
//!   own thread tells the player where it stopped and how to resume (its Continue button, or F8); it closes
//!   itself when that pause ends, and it does not take focus. The game window's title says the same. While
//!   paused, the transport's paused callbacks run on the game thread inside the hook, so its queued chunks
//!   still run.
//! * **The sink** is called with the debugger's lock held: it must only queue the frame, never call back
//!   into `m2_lua_debug_*`.

use crate::{hook, lua_host, luastack::Process, rt_log, sites};
use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use mercs2_formats::lua_debug::host::{give_up, handle, Step, Work};
use mercs2_formats::lua_debug::retail::{Retail, MASK_CALL, MASK_COUNT, MASK_RETURN};
use mercs2_formats::lua_debug::session::Debugger;
use mercs2_formats::lua_debug::{game_text, stamp, At, Event, HostKind, PauseReason, Request};
use mercs2_formats::lua_proto::{read_proto, source};
use mercs2_formats::lua_stack::Mem;
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F8};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumThreadWindows, GetMessageW, GetWindowTextW,
    IsWindowVisible, PostQuitMessage, RegisterClassW, SetTimer, SetWindowTextW, ShowWindow, TranslateMessage, BS_PUSHBUTTON,
    MSG, SW_SHOWNOACTIVATE, WM_COMMAND, WM_DESTROY, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

/// `int cb(unsigned conn, const char* json, int len, void* ud)`: deliver one frame to client `conn`; 0 when
/// it could not be delivered (the client is gone or its outbox is full).
pub type EventCb = unsafe extern "C" fn(conn: u32, json: *const c_char, len: c_int, ud: *mut c_void) -> c_int;
/// `void cb(void* L, void* ud)`: called repeatedly on the game thread while paused.
pub type PausedCb = unsafe extern "C" fn(l: *mut c_void, ud: *mut c_void);

const OFF_HOOKMASK: u32 = 0x36;
const OFF_BASEHOOKCOUNT: u32 = 0x38;
const OFF_HOOKCOUNT: u32 = 0x3C;
const OFF_HOOK: u32 = 0x40;
const AR_EVENT: u32 = 0x00;
const AR_I_CI: u32 = 0x60;
/// How often the pause loop looks for work and F8, and the notice for the end of its pause.
const PAUSE_POLL: Duration = Duration::from_millis(30);
const NOTICE_CLASS: &str = "M2DebugNotice";
const NOTICE_CONTINUE: usize = 1;

struct State {
    debugger: Debugger,
    retail: Option<Retail>,
    queue: VecDeque<Work>,
    /// States carrying the hook.
    hooked: Vec<u32>,
    /// Set by a step handled in the pause loop: leave it.
    resume: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static WAKE: Condvar = Condvar::new();
/// A client is attached: the detours act and new states get the hook.
static ATTACHED: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
/// Bumped at every pause; a notice belongs to one.
static PAUSE_GEN: AtomicU32 = AtomicU32::new(0);
/// The notice of pause [`PAUSE_GEN`] was dismissed by the player.
static PLAYER_RESUMED: AtomicU32 = AtomicU32::new(0);
static SINK: Mutex<Option<(usize, usize)>> = Mutex::new(None);
static PAUSED_CBS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
/// Every chunk loaded since install: lowercased source → (source, stamp).
static MODULES: Mutex<Option<HashMap<String, (String, String)>>> = Mutex::new(None);
static ORIG_UNDUMP: AtomicUsize = AtomicUsize::new(0);
static ORIG_POSCALL: AtomicUsize = AtomicUsize::new(0);
/// Why the debugger cannot run in this game, when its sites did not resolve.
static UNAVAILABLE: Mutex<Option<String>> = Mutex::new(None);
static BUILD: OnceLock<String> = OnceLock::new();
static BINDINGS: OnceLock<Result<HashMap<u32, String>, String>> = OnceLock::new();
/// Hook calls and the time spent in them since attach (paused time excluded), logged at detach.
static HOOK_CALLS: AtomicU64 = AtomicU64::new(0);
static HOOK_NANOS: AtomicU64 = AtomicU64::new(0);

fn lock() -> MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

fn write_u8(addr: u32, v: u8) {
    unsafe { (addr as *mut u8).write(v) }
}

fn write_u32(addr: u32, v: u32) {
    unsafe { (addr as *mut u32).write_unaligned(v) }
}

fn exe_path() -> Result<std::path::PathBuf, String> {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(core::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n == buf.len() {
        return Err("GetModuleFileNameW gave no path for the game executable".into());
    }
    Ok(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// The running executable's catalogued build and sha256, hashed once (on a transport thread).
fn build() -> &'static str {
    BUILD.get_or_init(|| {
        use sha2::{Digest, Sha256};
        let read = exe_path().and_then(|p| std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())));
        match read {
            Err(e) => format!("unknown build ({e})"),
            Ok(bytes) => {
                let sha: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
                let id = mercs2_exemap::Catalogue::embedded()
                    .ok()
                    .and_then(|c| c.build_by_sha(&sha).map(|b| b.id.clone()))
                    .unwrap_or_else(|| "uncatalogued".into());
                format!("{id} (sha256 {sha})")
            }
        }
    })
}

/// Every engine binding by function address, from the game's registered namespaces in its own executable
/// (`mercs2_exemap::luareg`, which reads the registry rows the exe carries as data).
fn bindings() -> &'static Result<HashMap<u32, String>, String> {
    BINDINGS.get_or_init(|| {
        let path = exe_path()?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let tables = mercs2_exemap::luareg::tables(&bytes).map_err(|e| format!("luaL_Reg tables of {}: {e}", path.display()))?;
        let mut map = HashMap::new();
        for t in tables {
            let Some(ns) = t.namespace else { continue };
            for (name, f) in t.entries {
                map.insert(f, format!("{ns}.{name}"));
            }
        }
        rt_log::line(&format!("debugger: {} engine bindings named from {}", map.len(), path.display()));
        Ok(map)
    })
}

/// Send `frames`; `Err` names the first that could not be delivered.
fn deliver(frames: &[(u64, Event)]) -> Result<(), String> {
    if frames.is_empty() {
        return Ok(());
    }
    let Some((cb, ud)) = *SINK.lock().unwrap_or_else(|p| p.into_inner()) else {
        return Err("no transport registered a sink".into());
    };
    let cb: EventCb = unsafe { core::mem::transmute(cb) };
    for (conn, e) in frames {
        let json = e.to_json();
        if unsafe { cb(*conn as u32, json.as_ptr().cast(), json.len() as c_int, ud as *mut c_void) } == 0 {
            return Err(format!(
                "client {conn} could not take {}",
                e.seq().map_or("an answer".into(), |s| format!("event {s}"))
            ));
        }
    }
    Ok(())
}

/// Send to the holder.
fn send(s: &mut State, events: Vec<Event>) {
    let Some(conn) = s.debugger.holder() else { return };
    let frames: Vec<(u64, Event)> = events.into_iter().map(|e| (conn, e)).collect();
    if let Err(e) = deliver(&frames) {
        fail(s, e);
    }
}

fn hook_addr() -> u32 {
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) = hook;
    f as usize as u32
}

/// Whether `l` carries the hook now. The game can close a state and build a new one at the same address,
/// and the new one starts with none.
fn carries_hook(l: u32) -> bool {
    Process.read_u32(l + OFF_HOOK) == hook_addr()
}

/// Put the hook on `l` while attached, and remember it; `why` says how it came to need it.
fn rehook(s: &mut State, l: u32, why: &str) {
    let Some(retail) = s.retail.as_mut() else { return };
    match retail.count_for(&Process, l, &s.debugger) {
        Ok(count) => {
            set_hook(l, true, count);
            if !s.hooked.contains(&l) {
                s.hooked.push(l);
            }
            rt_log::line(&format!("debugger: hook put on lua_State {l:#x} ({why})"));
        }
        Err(e) => fail(s, format!("lua_State {l:#x} ({why}) could not take the hook: {e}")),
    }
}

/// Write the hook into `l` (with the count hook or without), or take it out.
fn set_hook(l: u32, on: bool, count: bool) {
    if on {
        write_u32(l + OFF_HOOK, hook_addr());
        set_count(l, count);
    } else {
        write_u8(l + OFF_HOOKMASK, 0);
        write_u32(l + OFF_HOOK, 0);
        write_u32(l + OFF_BASEHOOKCOUNT, 0);
        write_u32(l + OFF_HOOKCOUNT, 0);
    }
}

fn set_count(l: u32, count: bool) {
    if count {
        write_u32(l + OFF_BASEHOOKCOUNT, 1);
        write_u32(l + OFF_HOOKCOUNT, 1);
        write_u8(l + OFF_HOOKMASK, MASK_CALL | MASK_RETURN | MASK_COUNT);
    } else {
        write_u8(l + OFF_HOOKMASK, MASK_CALL | MASK_RETURN);
    }
}

fn remove_hooks(s: &mut State) {
    for l in s.hooked.drain(..) {
        if lua_host::state_usable(l) {
            set_hook(l, false, false);
        }
    }
    s.retail = None;
    if ATTACHED.swap(false, Ordering::AcqRel) {
        let (calls, nanos) = (HOOK_CALLS.swap(0, Ordering::AcqRel), HOOK_NANOS.swap(0, Ordering::AcqRel));
        rt_log::line(&format!(
            "debugger: hook removed; it ran {calls} time(s) for {:.1} ms ({:.2} us each)",
            nanos as f64 / 1e6,
            if calls == 0 { 0.0 } else { nanos as f64 / calls as f64 / 1e3 }
        ));
    }
}

/// Carry out a step decided in `lua_debug::host`, on the game thread with `l` a running state.
fn apply(s: &mut State, step: Step, l: u32) {
    if let Err(e) = deliver(&step.sends) {
        return fail(s, e);
    }
    if step.remove {
        remove_hooks(s);
    }
    if step.resume {
        s.resume = true;
    }
    if step.install {
        install(s, l);
    }
    if step.refilter {
        refilter(s);
    }
}

/// The module filter changed: each hooked state's running frame decides its count hook again, and each
/// newly filtered module already loaded is reported.
fn refilter(s: &mut State) {
    let Some(retail) = s.retail.as_mut() else { return };
    for &st in &s.hooked {
        if !lua_host::state_usable(st) {
            continue;
        }
        match retail.count_for(&Process, st, &s.debugger) {
            Ok(count) => set_count(st, count),
            Err(e) => return fail(s, format!("lua_State {st:#x} after the filter changed: {e}")),
        }
    }
    let mut events = Vec::new();
    for (src, st) in MODULES.lock().unwrap_or_else(|p| p.into_inner()).iter().flat_map(|m| m.values()) {
        events.extend(s.debugger.module(src, st));
    }
    send(s, events);
}

/// Give the debugger up: the holder is told why if it can be, and the hook comes off.
fn fail(s: &mut State, why: String) {
    rt_log::line(&format!("debugger: giving up: {why}"));
    let step = give_up(&mut s.debugger, why);
    let _ = deliver(&step.sends);
    remove_hooks(s);
    s.resume = true;
}

fn install(s: &mut State, l: u32) {
    let names = match bindings() {
        Ok(n) => n.clone(),
        Err(e) => return fail(s, format!("engine binding names: {e}")),
    };
    let mut retail = Retail::new(names);
    s.hooked = lua_host::captured_states();
    for &st in &s.hooked {
        match retail.count_for(&Process, st, &s.debugger) {
            Ok(count) => set_hook(st, true, count),
            Err(e) => {
                s.retail = Some(retail);
                return fail(s, format!("lua_State {st:#x} at attach: {e}"));
            }
        }
    }
    HOOK_CALLS.store(0, Ordering::Release);
    HOOK_NANOS.store(0, Ordering::Release);
    ATTACHED.store(true, Ordering::Release);
    rt_log::line(&format!("debugger: attached; hook on {} state(s)", s.hooked.len()));
    let mut events = Vec::new();
    for (src, st) in MODULES.lock().unwrap_or_else(|p| p.into_inner()).iter().flat_map(|m| m.values()) {
        events.extend(s.debugger.module(src, st));
    }
    match retail.stack(&Process, l, &mut s.debugger) {
        Ok(ev) => events.extend(ev),
        Err(e) => {
            s.retail = Some(retail);
            return fail(s, format!("the stack at attach: {e}"));
        }
    }
    s.retail = Some(retail);
    send(s, events);
}

fn unavailable() -> Option<String> {
    UNAVAILABLE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Handle everything queued, on the game thread.
fn drain(s: &mut State, l: u32) {
    let why = unavailable();
    while let Some(w) = s.queue.pop_front() {
        let step = handle(&mut s.debugger, w, why.as_deref());
        apply(s, step, l);
    }
}

/// A state the Lua host captured: it gets the hook while a client is attached.
pub(crate) fn state_captured(l: u32) {
    if !ATTACHED.load(Ordering::Acquire) {
        return;
    }
    let mut g = lock();
    let Some(s) = g.as_mut() else { return };
    if s.hooked.contains(&l) && carries_hook(l) {
        return;
    }
    let why = if s.hooked.contains(&l) { "captured again without it" } else { "captured while attached" };
    rehook(s, l, why);
}

/// From the Lua host's tick on the game thread (never inside an executor chunk): put the hook back on a
/// state rebuilt at a hooked address, then handle queued work.
pub(crate) fn tick(l: u32) {
    if PAUSED.load(Ordering::Acquire) {
        return;
    }
    let mut g = lock();
    let Some(s) = g.as_mut() else { return };
    if ATTACHED.load(Ordering::Acquire) && !carries_hook(l) {
        rehook(s, l, "it no longer carried the hook: a state rebuilt at that address");
    }
    drain(s, l);
    s.resume = false;
}

fn hook_inner(l: u32, ar: u32) {
    if !ATTACHED.load(Ordering::Acquire) || lua_host::in_exec() {
        return;
    }
    let started = Instant::now();
    let (event, depth) = (Process.read_u32(ar + AR_EVENT), Process.read_u32(ar + AR_I_CI));
    let mut g = lock();
    let Some(s) = g.as_mut() else { return };
    let Some(retail) = s.retail.as_mut() else { return };
    let outcome = match retail.hook(&Process, l, event, depth, &mut s.debugger) {
        Ok(o) => o,
        Err(e) => return fail(s, format!("hook event {event} at depth {depth}: {e}")),
    };
    if let Some(count) = outcome.count {
        set_count(l, count);
    }
    send(s, outcome.events);
    HOOK_CALLS.fetch_add(1, Ordering::Relaxed);
    HOOK_NANOS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    if let Some((at, reason)) = outcome.pause
        && s.debugger.attached()
    {
        pause(g, l, at, reason);
    }
}

unsafe extern "C" fn hook(l: *mut c_void, ar: *mut c_void) {
    hook_inner(l as u32, ar as u32);
}

/// Block the game thread at `at` until a client or the player resumes it.
fn pause(mut g: MutexGuard<'static, Option<State>>, l: u32, at: At, reason: PauseReason) {
    let s = g.as_mut().expect("paused with a state");
    let ev = s.debugger.pause(at.clone(), reason).expect("attached");
    send(s, vec![ev]);
    if !s.debugger.attached() {
        return;
    }
    s.resume = false;
    PAUSED.store(true, Ordering::Release);
    let generation = PAUSE_GEN.fetch_add(1, Ordering::AcqRel) + 1;
    let place = format!("{}:{}", at.module, at.line.map_or("?".into(), |n| n.to_string()));
    rt_log::line(&format!("debugger: paused at {place} ({reason:?})"));
    let title = Title::set(&format!("PAUSED by the Workshop debugger at {place} - press F8 to continue"));
    show_notice(generation, &place);
    let mut f8_was_down = unsafe { GetAsyncKeyState(i32::from(VK_F8)) } as u16 & 0x8000 != 0;
    loop {
        let s = g.as_mut().expect("state outlives a pause");
        drain(s, l);
        if s.resume {
            break;
        }
        let f8 = unsafe { GetAsyncKeyState(i32::from(VK_F8)) } as u16 & 0x8000 != 0;
        let pressed = f8 && !f8_was_down;
        f8_was_down = f8;
        if pressed || PLAYER_RESUMED.load(Ordering::Acquire) == generation {
            rt_log::line(&format!("debugger: the player resumed at {place}"));
            break;
        }
        let cbs: Vec<(usize, usize)> = PAUSED_CBS.lock().unwrap_or_else(|p| p.into_inner()).clone();
        drop(g);
        for (cb, ud) in cbs {
            let cb: PausedCb = unsafe { core::mem::transmute(cb) };
            unsafe { cb(l as *mut c_void, ud as *mut c_void) };
        }
        g = lock();
        g = WAKE.wait_timeout(g, PAUSE_POLL).unwrap_or_else(|p| p.into_inner()).0;
    }
    PAUSED.store(false, Ordering::Release);
    drop(title);
    let s = g.as_mut().expect("state outlives a pause");
    s.resume = false;
    if let Some(ev) = s.debugger.resumed() {
        send(s, vec![ev]);
    }
}

/// The game window's title, replaced while paused and put back after. The game's windows belong to the
/// game thread, which is the thread that pauses, so `SetWindowTextW` does not wait on a blocked thread.
struct Title {
    hwnd: HWND,
    old: Vec<u16>,
}

impl Title {
    fn set(text: &str) -> Option<Title> {
        unsafe extern "system" fn first_visible(hwnd: HWND, out: LPARAM) -> i32 {
            if unsafe { IsWindowVisible(hwnd) } != 0 {
                unsafe { *(out as *mut HWND) = hwnd };
                return 0;
            }
            1
        }
        let mut hwnd: HWND = core::ptr::null_mut();
        unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(first_visible), &mut hwnd as *mut HWND as LPARAM) };
        if hwnd.is_null() {
            rt_log::line("debugger: the pausing thread owns no visible window; the title is unchanged");
            return None;
        }
        let mut old = vec![0u16; 512];
        let n = unsafe { GetWindowTextW(hwnd, old.as_mut_ptr(), old.len() as i32) } as usize;
        old.truncate(n);
        old.push(0);
        let new = wide(text);
        unsafe { SetWindowTextW(hwnd, new.as_ptr()) };
        Some(Title { hwnd, old })
    }
}

impl Drop for Title {
    fn drop(&mut self) {
        unsafe { SetWindowTextW(self.hwnd, self.old.as_ptr()) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// The notice window's procedure: Continue resumes its pause; the timer closes it once its pause is over.
unsafe extern "system" fn notice_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let generation = NOTICE_GEN.with(|g| g.get());
    match msg {
        WM_COMMAND if wparam & 0xFFFF == NOTICE_CONTINUE => {
            if PAUSED.load(Ordering::Acquire) && PAUSE_GEN.load(Ordering::Acquire) == generation {
                PLAYER_RESUMED.store(generation, Ordering::Release);
                WAKE.notify_all();
            }
            0
        }
        WM_TIMER => {
            if !PAUSED.load(Ordering::Acquire) || PAUSE_GEN.load(Ordering::Acquire) != generation {
                unsafe { DestroyWindow(hwnd) };
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

thread_local! {
    /// The pause the notice on this thread belongs to.
    static NOTICE_GEN: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

/// The player's notice for pause `generation`, on its own thread: a topmost window that does not take focus,
/// with a Continue button; it closes itself when that pause ends however it ends.
fn show_notice(generation: u32, place: &str) {
    let text = format!(
        "The game is paused by the Workshop debugger at {place}.\r\nPress Continue or F8 to continue, or click Continue in the Workshop."
    );
    std::thread::spawn(move || {
        NOTICE_GEN.with(|g| g.set(generation));
        let instance = unsafe { GetModuleHandleW(core::ptr::null()) };
        let class = wide(NOTICE_CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(notice_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..unsafe { core::mem::zeroed() }
        };
        // A second registration fails harmlessly with the class already registered.
        unsafe { RegisterClassW(&wc) };
        let (title, body, button, static_class, button_class) =
            (wide("Mercenaries 2: paused"), wide(&text), wide("Continue"), wide("STATIC"), wide("BUTTON"));
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP | WS_CAPTION | WS_BORDER | WS_VISIBLE,
                40,
                40,
                520,
                150,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                instance,
                core::ptr::null(),
            )
        };
        if hwnd.is_null() {
            rt_log::line("debugger: the pause notice window could not be created");
            return;
        }
        unsafe {
            CreateWindowExW(0, static_class.as_ptr(), body.as_ptr(), WS_CHILD | WS_VISIBLE, 12, 10, 490, 60, hwnd, core::ptr::null_mut(), instance, core::ptr::null());
            CreateWindowExW(
                0,
                button_class.as_ptr(),
                button.as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
                390,
                76,
                110,
                28,
                hwnd,
                NOTICE_CONTINUE as _,
                instance,
                core::ptr::null(),
            );
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetTimer(hwnd, 1, PAUSE_POLL.as_millis() as u32, None);
        }
        let mut msg: MSG = unsafe { core::mem::zeroed() };
        while unsafe { GetMessageW(&mut msg, core::ptr::null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

/// After `luaU_undump` built a chunk's main function: stamp it, and report it to an attached client.
fn chunk_loaded(main: u32) {
    let read = source(&Process, main).and_then(|src| read_proto(&Process, main).map(|p| (game_text(&src), stamp(&p))));
    let (src, st) = match read {
        Ok(v) => v,
        Err(e) => {
            rt_log::line(&format!("debugger: the chunk loaded at Proto {main:#x} could not be read: {e}"));
            return;
        }
    };
    MODULES
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(src.to_ascii_lowercase(), (src.clone(), st.clone()));
    if !ATTACHED.load(Ordering::Acquire) {
        return;
    }
    let mut g = lock();
    let Some(s) = g.as_mut() else { return };
    let events: Vec<Event> = s.debugger.module(&src, &st).into_iter().collect();
    send(s, events);
}

unsafe extern "C" fn undump_detour(l: u32, z: u32, buff: u32, name: u32) -> u32 {
    let orig: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = unsafe { core::mem::transmute(ORIG_UNDUMP.load(Ordering::Acquire)) };
    let main = unsafe { orig(l, z, buff, name) };
    chunk_loaded(main);
    main
}

/// From the `luaD_poscall` detour, before the original runs: an engine call's results.
extern "C" fn poscall_seen(l: u32, first_result: u32) {
    if !ATTACHED.load(Ordering::Acquire) || lua_host::in_exec() {
        return;
    }
    let mut g = lock();
    let Some(s) = g.as_mut() else { return };
    let Some(retail) = s.retail.as_mut() else { return };
    match retail.poscall(&Process, l, first_result, &mut s.debugger) {
        Ok(ev) => send(s, ev.into_iter().collect()),
        Err(e) => fail(s, format!("luaD_poscall: {e}")),
    }
}

/// `luaD_poscall` takes L in `ECX` and the first result in `EAX`: save the registers, report, restore them
/// and continue into the original with its return address untouched.
#[unsafe(naked)]
unsafe extern "C" fn poscall_detour() {
    core::arch::naked_asm!(
        "push eax",
        "push ecx",
        "push edx",
        "push eax",
        "push ecx",
        "call {seen}",
        "add esp, 8",
        "pop edx",
        "pop ecx",
        "pop eax",
        "jmp dword ptr [{orig}]",
        seen = sym poscall_seen,
        orig = sym ORIG_POSCALL,
    )
}

/// Resolve the debugger's sites and detour them; called from the Lua host's install. A failure leaves the
/// debugger unavailable (every attach is refused with the reason) and the Lua host working.
pub(crate) fn install_detours() {
    let attach = |what: &str, at: u32, detour: *mut c_void, orig: &AtomicUsize| -> Result<(), String> {
        if Process.read_u8(at) == 0xe9 {
            return Err(format!("{what} at {at:#x} is already detoured by another module"));
        }
        if !unsafe { hook::attach(at as *mut c_void, detour, orig.as_ptr().cast()) } {
            return Err(format!("MinHook could not detour {what} at {at:#x}"));
        }
        rt_log::line(&format!("debugger: detoured {what} at {at:#x}"));
        Ok(())
    };
    let result = sites::debug().and_then(|d| {
        attach("luaU_undump", d.undump, undump_detour as *mut c_void, &ORIG_UNDUMP)?;
        attach("luaD_poscall", d.poscall, poscall_detour as *mut c_void, &ORIG_POSCALL)
    });
    if let Err(e) = result {
        rt_log::line(&format!("ERROR debugger unavailable: {e}"));
        *UNAVAILABLE.lock().unwrap_or_else(|p| p.into_inner()) = Some(format!("the debugger is unavailable in this game: {e}"));
    }
}

/// Register the transport's sink, which every event and answer goes through. One per process: 0 when one is
/// already registered or `cb` is NULL.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_debug_set_sink(cb: Option<EventCb>, ud: *mut c_void) -> c_int {
    let Some(cb) = cb else { return 0 };
    let mut sink = SINK.lock().unwrap_or_else(|p| p.into_inner());
    if sink.is_some() {
        return 0;
    }
    *sink = Some((cb as usize, ud as usize));
    1
}

/// Call `cb(L, ud)` on the game thread over and over while it is paused (where queued chunks can run).
/// 0 when `cb` is NULL.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_debug_on_paused(cb: Option<PausedCb>, ud: *mut c_void) -> c_int {
    let Some(cb) = cb else { return 0 };
    PAUSED_CBS.lock().unwrap_or_else(|p| p.into_inner()).push((cb as usize, ud as usize));
    1
}

/// 1 while the game thread is paused by the debugger.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_debug_paused() -> c_int {
    c_int::from(PAUSED.load(Ordering::Acquire))
}

/// Queue one debug request (`len` bytes of a `debug.*` JSON frame) from client `conn`, for the game thread.
/// 1 when queued; 0 when it is not a valid debug request, with the reason in `m2_lua_last_error`.
///
/// # Safety
/// `json` is readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_debug_request(conn: u32, json: *const c_char, len: c_int) -> c_int {
    if json.is_null() || len < 0 {
        lua_host::record_error("m2_lua_debug_request: no frame".into());
        return 0;
    }
    let body = unsafe { core::slice::from_raw_parts(json.cast::<u8>(), len as usize) };
    let req = match Request::parse(body) {
        Ok(Some(r)) => r,
        Ok(None) => {
            lua_host::record_error("m2_lua_debug_request: the frame has no `type`; it is a REPL request".into());
            return 0;
        }
        Err(e) => {
            lua_host::record_error(format!("m2_lua_debug_request: {e}"));
            return 0;
        }
    };
    // Read the executable on this thread, not the game's: its build, and the binding names an engine
    // breakpoint can name.
    let build = build().to_owned();
    let known = bindings();
    let mut g = lock();
    let s = g.get_or_insert_with(|| {
        let mut debugger = Debugger::new(HostKind::Retail, build);
        debugger.set_bindings(known.as_ref().map(|m| m.values().cloned().collect()).map_err(String::clone));
        State { debugger, retail: None, queue: VecDeque::new(), hooked: Vec::new(), resume: false }
    });
    s.queue.push_back(Work::Request(u64::from(conn), req));
    WAKE.notify_all();
    1
}

/// Client `conn`'s connection closed: if it held the debugger, the hook comes off and a paused game resumes.
#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_debug_disconnected(conn: u32) {
    let mut g = lock();
    if let Some(s) = g.as_mut() {
        s.queue.push_back(Work::Disconnected(u64::from(conn)));
        WAKE.notify_all();
    }
}
