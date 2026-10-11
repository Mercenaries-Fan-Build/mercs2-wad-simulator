//! The shared subscription to the game's log stream (`m2/m2_loghook.h`). One source for the process,
//! fanned out to up to 16 listeners. The source, best first:
//! - pmc_bb exports `pmc_log_subscribe`: subscribe to its in-process stream;
//! - an older pmc_bb owns the log stub and writes `pmc_blackbox.log`: tail that file;
//! - no pmc_bb: detour the shared log stub ourselves and chain to the trampoline.

use crate::{hook, log, luastack};
use core::ffi::{CStr, c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicUsize, Ordering};
use mercs2_formats::{exe_target as target, lua_stack as core_stack};
use windows_sys::Win32::Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileSizeEx,
    OPEN_EXISTING, ReadFile, SetFilePointerEx,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows_sys::Win32::System::Threading::Sleep;

/// Called on the thread that produced the line, with the message NUL-terminated.
pub type LogListener = unsafe extern "C" fn(msg: *const c_char, ud: *mut c_void);
type LogStubFn = unsafe extern "C" fn(l: *mut c_void) -> c_int;
type PmcLogSubscribe = unsafe extern "C" fn(cb: LogListener, ud: *mut c_void) -> c_int;

const MAX_LISTENERS: usize = 16;
const TAIL_POLL_MS: u32 = 300;
const TAIL_WAIT_MS: u32 = 20_000;
const TAIL_LINE_MAX: usize = 2048;
const MSG_MAX: usize = 2048;

/// `cb` is published last and read first, so a dispatcher skips a slot another thread has only claimed.
struct Listener {
    cb: AtomicPtr<c_void>,
    ud: AtomicPtr<c_void>,
}

static LISTENERS: [Listener; MAX_LISTENERS] =
    [const { Listener { cb: AtomicPtr::new(core::ptr::null_mut()), ud: AtomicPtr::new(core::ptr::null_mut()) } };
        MAX_LISTENERS];
static LISTENER_COUNT: AtomicI32 = AtomicI32::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static IN_HOOK: AtomicBool = AtomicBool::new(false);
static ORIG_STUB: AtomicUsize = AtomicUsize::new(0);

/// Register a listener; 0 when `cb` is null or all 16 slots are taken. The slot claim is lock-free
/// because a mod calls this from its `DllMain`, under the loader lock. The count is never rolled back on
/// overflow, so no slot is handed out twice.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loghook_add_listener(cb: Option<LogListener>, ud: *mut c_void) -> c_int {
    let Some(cb) = cb else { return 0 };
    let slot = LISTENER_COUNT.fetch_add(1, Ordering::AcqRel);
    let Some(l) = usize::try_from(slot).ok().and_then(|s| LISTENERS.get(s)) else { return 0 };
    l.ud.store(ud, Ordering::Relaxed);
    l.cb.store(cb as *mut c_void, Ordering::Release);
    1
}

fn dispatch(msg: &CStr) {
    let n = (LISTENER_COUNT.load(Ordering::Acquire).max(0) as usize).min(MAX_LISTENERS);
    for l in &LISTENERS[..n] {
        let cb = l.cb.load(Ordering::Acquire);
        if cb.is_null() {
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

/// Follow `pmc_blackbox.log` next to the exe from its start, re-reading from 0 when it is truncated.
fn tail() {
    let mut path = log::module_dir(core::ptr::null_mut());
    path.extend_from_slice(b"pmc_blackbox.log\0");
    let mut waited = 0;
    let h = loop {
        let h = unsafe {
            CreateFileA(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                core::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                core::ptr::null_mut(),
            )
        };
        if h != INVALID_HANDLE_VALUE {
            break h;
        }
        if waited >= TAIL_WAIT_MS {
            return;
        }
        unsafe { Sleep(TAIL_POLL_MS) };
        waited += TAIL_POLL_MS;
    };
    let mut off: i64 = 0;
    let mut line: Vec<u8> = Vec::with_capacity(TAIL_LINE_MAX);
    loop {
        let mut size: i64 = 0;
        if unsafe { GetFileSizeEx(h, &mut size) } != 0 {
            if size < off {
                off = 0;
                line.clear();
            }
            if size > off {
                let mut buf = [0u8; 1024];
                unsafe { SetFilePointerEx(h, off, core::ptr::null_mut(), FILE_BEGIN) };
                loop {
                    let mut got = 0u32;
                    let ok = unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut got, core::ptr::null_mut()) };
                    if ok == 0 || got == 0 {
                        break;
                    }
                    off += i64::from(got);
                    for &c in &buf[..got as usize] {
                        if c == b'\n' || c == b'\r' {
                            if !line.is_empty() {
                                dispatch_bytes(&line);
                                line.clear();
                            }
                        } else if line.len() < TAIL_LINE_MAX - 1 {
                            line.push(c);
                        }
                    }
                    if (got as usize) < buf.len() {
                        break;
                    }
                }
            }
        }
        unsafe { Sleep(TAIL_POLL_MS) };
    }
}

/// The detour on the shared log stub. Only Lua `print` / `Debug.Printf` reach it with a real
/// `lua_State`; the join rejects every other caller. It always chains to the trampoline, which holds
/// pmc_bb's jump when pmc_bb hooked the stub first.
unsafe extern "C" fn hook_log_stub(l: *mut c_void) -> c_int {
    if IN_HOOK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        let mut msg = [0u8; MSG_MAX];
        if core_stack::join_strings(&luastack::Process, l as u32, &mut msg) >= 1 {
            dispatch(CStr::from_bytes_until_nul(&msg).expect("join_strings terminates"));
        }
        IN_HOOK.store(false, Ordering::Release);
    }
    let orig = ORIG_STUB.load(Ordering::Acquire);
    if orig == 0 {
        return 0;
    }
    let orig: LogStubFn = unsafe { core::mem::transmute(orig) };
    unsafe { orig(l) }
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

/// Start the log source. Idempotent; 1 on success.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loghook_install() -> c_int {
    if INSTALLED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return 1;
    }
    let bb = unsafe { GetModuleHandleA(c"pmc_bb.dll".as_ptr().cast()) };
    if !bb.is_null() {
        if let Some(sub) = unsafe { GetProcAddress(bb, c"pmc_log_subscribe".as_ptr().cast()) } {
            let sub: PmcLogSubscribe = unsafe { core::mem::transmute(sub) };
            if unsafe { sub(on_pmc_line, core::ptr::null_mut()) } != 0 {
                return 1;
            }
        }
        if std::thread::Builder::new().name("m2_loghook_tail".into()).spawn(tail).is_ok() {
            return 1;
        }
    }
    let attached = unsafe {
        hook::attach(target::LOG_STUB_VA as *mut c_void, hook_log_stub as *mut c_void, ORIG_STUB.as_ptr().cast())
    };
    if !attached {
        INSTALLED.store(false, Ordering::Release);
        return 0;
    }
    1
}
