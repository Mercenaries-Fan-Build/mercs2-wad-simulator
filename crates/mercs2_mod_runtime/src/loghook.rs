//! The shared subscription to the game's log stream (`m2/m2_loghook.h`). One source for the process,
//! fanned out to up to 16 listeners:
//! - pmc_bb exports `pmc_log_subscribe`: subscribe to its in-process stream;
//! - otherwise: the shared log-stub detour ([`crate::stub`]), found by name in the game image. When pmc_bb
//!   has hooked the stub first, the trampoline runs its detour, so both see every line.
//!   `pmc_blackbox.log` is not a source: an older pmc_bb writes the game's lines to it only with
//!   `PMC_VERBOSE_LOG` set, so following it left the listeners without a line, and load triggers never
//!   fired, whenever that variable was unset.

use crate::{luastack, rt_log, stub};
use core::ffi::{CStr, c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use mercs2_formats::lua_stack as core_stack;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

/// Called on the thread that produced the line, with the message NUL-terminated.
pub type LogListener = unsafe extern "C" fn(msg: *const c_char, ud: *mut c_void);
type PmcLogSubscribe = unsafe extern "C" fn(cb: LogListener, ud: *mut c_void) -> c_int;

const MAX_LISTENERS: usize = 16;
const MSG_MAX: usize = 2048;

/// `cb` is published last and read first, so a dispatcher skips a slot another thread has only claimed.
/// A paused listener (`active` false) is skipped.
struct Listener {
    cb: AtomicPtr<c_void>,
    ud: AtomicPtr<c_void>,
    active: AtomicBool,
}

static LISTENERS: [Listener; MAX_LISTENERS] = [const {
    Listener {
        cb: AtomicPtr::new(core::ptr::null_mut()),
        ud: AtomicPtr::new(core::ptr::null_mut()),
        active: AtomicBool::new(false),
    }
}; MAX_LISTENERS];
static LISTENER_COUNT: AtomicI32 = AtomicI32::new(0);
/// Listeners registered and not paused. With none, the stub source does not join the line at all: the
/// stub runs thousands of times a frame.
static ACTIVE_COUNT: AtomicI32 = AtomicI32::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static IN_HOOK: AtomicBool = AtomicBool::new(false);

/// Register a listener; 0 when `cb` is null or all 16 slots are taken. The slot claim is lock-free
/// because a mod calls this from its `DllMain`, under the loader lock. The count is never rolled back on
/// overflow, so no slot is handed out twice.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loghook_add_listener(cb: Option<LogListener>, ud: *mut c_void) -> c_int {
    let Some(cb) = cb else { return 0 };
    let slot = LISTENER_COUNT.fetch_add(1, Ordering::AcqRel);
    let Some(l) = usize::try_from(slot).ok().and_then(|s| LISTENERS.get(s)) else { return 0 };
    l.ud.store(ud, Ordering::Relaxed);
    l.active.store(true, Ordering::Relaxed);
    l.cb.store(cb as *mut c_void, Ordering::Release);
    ACTIVE_COUNT.fetch_add(1, Ordering::AcqRel);
    1
}

/// Pause (`active` 0) or resume a registered listener, found by `cb` and `ud`; 0 when none matches. A
/// paused listener gets no lines, and while every listener is paused the stub source costs one load per
/// call.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loghook_set_active(cb: Option<LogListener>, ud: *mut c_void, active: c_int) -> c_int {
    let Some(cb) = cb else { return 0 };
    let n = (LISTENER_COUNT.load(Ordering::Acquire).max(0) as usize).min(MAX_LISTENERS);
    let Some(l) = LISTENERS[..n]
        .iter()
        .find(|l| l.cb.load(Ordering::Acquire) == cb as *mut c_void && l.ud.load(Ordering::Relaxed) == ud)
    else {
        return 0;
    };
    let on = active != 0;
    if l.active.swap(on, Ordering::AcqRel) != on {
        ACTIVE_COUNT.fetch_add(if on { 1 } else { -1 }, Ordering::AcqRel);
    }
    1
}

fn dispatch(msg: &CStr) {
    let n = (LISTENER_COUNT.load(Ordering::Acquire).max(0) as usize).min(MAX_LISTENERS);
    for l in &LISTENERS[..n] {
        let cb = l.cb.load(Ordering::Acquire);
        if cb.is_null() || !l.active.load(Ordering::Acquire) {
            continue;
        }
        let cb: LogListener = unsafe { core::mem::transmute(cb) };
        unsafe { cb(msg.as_ptr(), l.ud.load(Ordering::Relaxed)) };
    }
}

/// Dispatch `bytes` up to their first NUL as one line.
fn dispatch_bytes(bytes: &[u8]) {
    let mut line = Vec::with_capacity(bytes.len() + 1);
    line.extend_from_slice(bytes);
    line.push(0);
    dispatch(CStr::from_bytes_until_nul(&line).expect("terminated above"));
}

/// One call of the shared log stub. Only Lua `print` / `Debug.Printf` reach it with a real `lua_State`;
/// the join rejects every other caller.
pub(crate) fn on_stub(l: *mut c_void) {
    if ACTIVE_COUNT.load(Ordering::Relaxed) <= 0 {
        return;
    }
    if IN_HOOK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        let mut msg = [0u8; MSG_MAX];
        if core_stack::join_strings(&luastack::Process, l as u32, &mut msg) >= 1 {
            dispatch(CStr::from_bytes_until_nul(&msg).expect("join_strings terminates"));
        }
        IN_HOOK.store(false, Ordering::Release);
    }
}

/// pmc_bb's line, without its trailing newline, capped at 2047 bytes.
unsafe extern "C" fn on_pmc_line(line: *const c_char, _ud: *mut c_void) {
    if line.is_null() {
        return;
    }
    let bytes = unsafe { CStr::from_ptr(line) }.to_bytes();
    let n = bytes.len() - bytes.iter().rev().take_while(|&&b| b == b'\n' || b == b'\r').count();
    if n == 0 {
        return;
    }
    dispatch_bytes(&bytes[..n.min(MSG_MAX - 1)]);
}

/// Start the log source. Idempotent; 1 on success. Without `pmc_log_subscribe` the stub detour is attached
/// now, or by a thread once SecuROM has decrypted the game's code (m2-sdk.log records the outcome).
#[unsafe(no_mangle)]
pub extern "C" fn m2_loghook_install() -> c_int {
    if INSTALLED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return 1;
    }
    let bb = unsafe { GetModuleHandleA(c"pmc_bb.dll".as_ptr().cast()) };
    if !bb.is_null()
        && let Some(sub) = unsafe { GetProcAddress(bb, c"pmc_log_subscribe".as_ptr().cast()) }
    {
        let sub: PmcLogSubscribe = unsafe { core::mem::transmute(sub) };
        if unsafe { sub(on_pmc_line, core::ptr::null_mut()) } != 0 {
            return 1;
        }
    }
    stub::LOG_ON.store(true, Ordering::Release);
    if let Err(e) = stub::attach_soon() {
        rt_log::line(&format!("ERROR m2_loghook_install: {e}"));
        stub::LOG_ON.store(false, Ordering::Release);
        INSTALLED.store(false, Ordering::Release);
        return 0;
    }
    1
}
